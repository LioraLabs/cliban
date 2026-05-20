package store

import (
	"database/sql"
	"errors"
	"fmt"
	"regexp"
	"strings"
	"time"

	"github.com/alex/cliban/internal/domain"
)

var projectKeyRE = regexp.MustCompile(`^[A-Z][A-Z0-9]{1,9}$`)

func validateProjectKey(k string) error {
	if !projectKeyRE.MatchString(k) {
		return fmt.Errorf("%w: project key must be 2-10 chars, uppercase letters/digits, starting with a letter", ErrValidation)
	}
	return nil
}

func (s *Store) CreateProject(key, name, description string) (*domain.Project, error) {
	if err := validateProjectKey(key); err != nil {
		return nil, err
	}
	if name == "" {
		return nil, fmt.Errorf("%w: name required", ErrValidation)
	}
	now := s.nowISO()
	res, err := s.db.Exec(`INSERT INTO project(key,name,description,archived,issue_seq,created_at,updated_at) VALUES(?,?,?,0,0,?,?)`,
		key, name, description, now, now)
	if err != nil {
		if isUniqueErr(err) {
			return nil, fmt.Errorf("%w: project %q already exists", ErrConflict, key)
		}
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	id, _ := res.LastInsertId()
	return s.GetProjectByID(id)
}

const projectSelectCols = `id,key,name,description,archived,issue_seq,auto_archive_done_after_days,created_at,updated_at`

func (s *Store) GetProjectByKey(key string) (*domain.Project, error) {
	row := s.db.QueryRow(`SELECT `+projectSelectCols+` FROM project WHERE key = ?`, key)
	return scanProject(row)
}

func (s *Store) GetProjectByID(id int64) (*domain.Project, error) {
	row := s.db.QueryRow(`SELECT `+projectSelectCols+` FROM project WHERE id = ?`, id)
	return scanProject(row)
}

func scanProject(r interface{ Scan(...any) error }) (*domain.Project, error) {
	var p domain.Project
	var archived int
	var autoArchive sql.NullInt64
	var created, updated string
	if err := r.Scan(&p.ID, &p.Key, &p.Name, &p.Description, &archived, &p.IssueSeq, &autoArchive, &created, &updated); err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return nil, ErrNotFound
		}
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	p.Archived = archived != 0
	if autoArchive.Valid {
		v := autoArchive.Int64
		p.AutoArchiveDoneAfterDays = &v
	}
	p.CreatedAt, _ = time.Parse(time.RFC3339Nano, created)
	p.UpdatedAt, _ = time.Parse(time.RFC3339Nano, updated)
	return &p, nil
}

func (s *Store) ListProjects(includeArchived bool) ([]*domain.Project, error) {
	q := `SELECT ` + projectSelectCols + ` FROM project`
	if !includeArchived {
		q += ` WHERE archived = 0`
	}
	q += ` ORDER BY key`
	rows, err := s.db.Query(q)
	if err != nil {
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	defer rows.Close()
	var out []*domain.Project
	for rows.Next() {
		p, err := scanProject(rows)
		if err != nil {
			return nil, err
		}
		out = append(out, p)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	return out, nil
}

func (s *Store) UpdateProject(key, name, description string) error {
	if name == "" {
		return fmt.Errorf("%w: name required", ErrValidation)
	}
	res, err := s.db.Exec(`UPDATE project SET name=?, description=?, updated_at=? WHERE key=?`,
		name, description, s.nowISO(), key)
	if err != nil {
		return fmt.Errorf("%w: %v", ErrInternal, err)
	}
	if n, _ := res.RowsAffected(); n == 0 {
		return ErrNotFound
	}
	return nil
}

// SetAutoArchiveDoneAfter sets (or clears, when days <= 0) the
// auto-archive-after-N-days policy for a project.
func (s *Store) SetAutoArchiveDoneAfter(key string, days int64) error {
	var v any
	if days > 0 {
		v = days
	}
	res, err := s.db.Exec(`UPDATE project SET auto_archive_done_after_days=?, updated_at=? WHERE key=?`, v, s.nowISO(), key)
	if err != nil {
		return fmt.Errorf("%w: %v", ErrInternal, err)
	}
	if n, _ := res.RowsAffected(); n == 0 {
		return ErrNotFound
	}
	return nil
}

// SweepAutoArchive archives done issues whose completed_at is older than the
// configured policy for each project. Returns the total count archived.
func (s *Store) SweepAutoArchive() (int, error) {
	rows, err := s.db.Query(`SELECT key, auto_archive_done_after_days FROM project WHERE auto_archive_done_after_days IS NOT NULL`)
	if err != nil {
		return 0, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	type pol struct {
		key  string
		days int64
	}
	var pols []pol
	for rows.Next() {
		var p pol
		if err := rows.Scan(&p.key, &p.days); err != nil {
			rows.Close()
			return 0, fmt.Errorf("%w: %v", ErrInternal, err)
		}
		pols = append(pols, p)
	}
	rows.Close()
	total := 0
	for _, p := range pols {
		// SQLite datetime arithmetic: completed_at + N days < now (UTC).
		res, err := s.db.Exec(`UPDATE issue
			SET archived = 1, updated_at = ?
			WHERE archived = 0
			  AND status = 'done'
			  AND completed_at IS NOT NULL
			  AND project_id = (SELECT id FROM project WHERE key = ?)
			  AND datetime(completed_at, '+' || ? || ' days') < datetime('now')`,
			s.nowISO(), p.key, p.days)
		if err != nil {
			return total, fmt.Errorf("%w: %v", ErrInternal, err)
		}
		n, _ := res.RowsAffected()
		total += int(n)
	}
	return total, nil
}

func (s *Store) SetProjectArchived(key string, archived bool) error {
	v := 0
	if archived {
		v = 1
	}
	res, err := s.db.Exec(`UPDATE project SET archived=?, updated_at=? WHERE key=?`, v, s.nowISO(), key)
	if err != nil {
		return fmt.Errorf("%w: %v", ErrInternal, err)
	}
	if n, _ := res.RowsAffected(); n == 0 {
		return ErrNotFound
	}
	return nil
}

func (s *Store) DeleteProject(key string) error {
	res, err := s.db.Exec(`DELETE FROM project WHERE key=?`, key)
	if err != nil {
		return fmt.Errorf("%w: %v", ErrInternal, err)
	}
	if n, _ := res.RowsAffected(); n == 0 {
		return ErrNotFound
	}
	return nil
}

func isUniqueErr(err error) bool {
	if err == nil {
		return false
	}
	msg := err.Error()
	return strings.Contains(msg, "UNIQUE constraint failed") || strings.Contains(msg, "constraint failed: UNIQUE")
}
