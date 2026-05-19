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

func (s *Store) GetProjectByKey(key string) (*domain.Project, error) {
	return s.queryProject(`WHERE key = ?`, key)
}

func (s *Store) GetProjectByID(id int64) (*domain.Project, error) {
	return s.queryProject(`WHERE id = ?`, id)
}

func (s *Store) queryProject(where string, args ...any) (*domain.Project, error) {
	row := s.db.QueryRow(`SELECT id,key,name,description,archived,issue_seq,created_at,updated_at FROM project `+where, args...)
	return scanProject(row)
}

func scanProject(r interface{ Scan(...any) error }) (*domain.Project, error) {
	var p domain.Project
	var archived int
	var created, updated string
	if err := r.Scan(&p.ID, &p.Key, &p.Name, &p.Description, &archived, &p.IssueSeq, &created, &updated); err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return nil, ErrNotFound
		}
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	p.Archived = archived != 0
	p.CreatedAt, _ = time.Parse(time.RFC3339Nano, created)
	p.UpdatedAt, _ = time.Parse(time.RFC3339Nano, updated)
	return &p, nil
}

func (s *Store) ListProjects(includeArchived bool) ([]*domain.Project, error) {
	q := `SELECT id,key,name,description,archived,issue_seq,created_at,updated_at FROM project`
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
