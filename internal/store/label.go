package store

import (
	"database/sql"
	"errors"
	"fmt"
	"strings"

	"github.com/alex/cliban/internal/domain"
)

// CreateLabel creates a label in a project. Returns ErrConflict if it
// already exists.
func (s *Store) CreateLabel(projectKey, name string) error {
	if name == "" {
		return fmt.Errorf("%w: label name required", ErrValidation)
	}
	proj, err := s.GetProjectByKey(strings.ToUpper(projectKey))
	if err != nil {
		return err
	}
	if _, err := s.db.Exec(`INSERT INTO label(project_id,name,created_at) VALUES(?,?,?)`, proj.ID, name, s.nowISO()); err != nil {
		if isUniqueErr(err) {
			return fmt.Errorf("%w: label %q already exists in %s", ErrConflict, name, proj.Key)
		}
		return fmt.Errorf("%w: %v", ErrInternal, err)
	}
	return nil
}

// ListLabels lists every label in a project.
func (s *Store) ListLabels(projectKey string) ([]string, error) {
	proj, err := s.GetProjectByKey(strings.ToUpper(projectKey))
	if err != nil {
		return nil, err
	}
	rows, err := s.db.Query(`SELECT name FROM label WHERE project_id=? ORDER BY name`, proj.ID)
	if err != nil {
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	defer rows.Close()
	var out []string
	for rows.Next() {
		var n string
		if err := rows.Scan(&n); err != nil {
			return nil, fmt.Errorf("%w: %v", ErrInternal, err)
		}
		out = append(out, n)
	}
	return out, rows.Err()
}

// DeleteLabel removes a label from a project (and all attachments).
func (s *Store) DeleteLabel(projectKey, name string) error {
	proj, err := s.GetProjectByKey(strings.ToUpper(projectKey))
	if err != nil {
		return err
	}
	res, err := s.db.Exec(`DELETE FROM label WHERE project_id=? AND name=?`, proj.ID, name)
	if err != nil {
		return fmt.Errorf("%w: %v", ErrInternal, err)
	}
	if n, _ := res.RowsAffected(); n == 0 {
		return ErrNotFound
	}
	return nil
}

// AttachLabel attaches a label (by name) to an issue, creating the label
// row if it does not exist.
func (s *Store) AttachLabel(k domain.IssueKey, name string) error {
	if name == "" {
		return fmt.Errorf("%w: label name required", ErrValidation)
	}
	tx, err := s.db.Begin()
	if err != nil {
		return fmt.Errorf("%w: %v", ErrInternal, err)
	}
	defer tx.Rollback()
	var projID, issueID int64
	if err := tx.QueryRow(`SELECT p.id, i.id FROM issue i JOIN project p ON p.id=i.project_id WHERE p.key=? AND i.seq=?`, k.Project, k.Seq).Scan(&projID, &issueID); err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return ErrNotFound
		}
		return fmt.Errorf("%w: %v", ErrInternal, err)
	}
	var labelID int64
	err = tx.QueryRow(`SELECT id FROM label WHERE project_id=? AND name=?`, projID, name).Scan(&labelID)
	if errors.Is(err, sql.ErrNoRows) {
		res, ierr := tx.Exec(`INSERT INTO label(project_id,name,created_at) VALUES(?,?,?)`, projID, name, s.nowISO())
		if ierr != nil {
			return fmt.Errorf("%w: %v", ErrInternal, ierr)
		}
		labelID, _ = res.LastInsertId()
	} else if err != nil {
		return fmt.Errorf("%w: %v", ErrInternal, err)
	}
	if _, err := tx.Exec(`INSERT OR IGNORE INTO issue_label(issue_id,label_id) VALUES(?,?)`, issueID, labelID); err != nil {
		return fmt.Errorf("%w: %v", ErrInternal, err)
	}
	return tx.Commit()
}

// DetachLabel removes a label from an issue (does not delete the label row).
func (s *Store) DetachLabel(k domain.IssueKey, name string) error {
	tx, err := s.db.Begin()
	if err != nil {
		return fmt.Errorf("%w: %v", ErrInternal, err)
	}
	defer tx.Rollback()
	var projID, issueID int64
	if err := tx.QueryRow(`SELECT p.id, i.id FROM issue i JOIN project p ON p.id=i.project_id WHERE p.key=? AND i.seq=?`, k.Project, k.Seq).Scan(&projID, &issueID); err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return ErrNotFound
		}
		return fmt.Errorf("%w: %v", ErrInternal, err)
	}
	var labelID int64
	if err := tx.QueryRow(`SELECT id FROM label WHERE project_id=? AND name=?`, projID, name).Scan(&labelID); err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return ErrNotFound
		}
		return fmt.Errorf("%w: %v", ErrInternal, err)
	}
	if _, err := tx.Exec(`DELETE FROM issue_label WHERE issue_id=? AND label_id=?`, issueID, labelID); err != nil {
		return fmt.Errorf("%w: %v", ErrInternal, err)
	}
	return tx.Commit()
}

// LabelsForIssue returns the label names attached to an issue (sorted).
func (s *Store) LabelsForIssue(issueID int64) ([]string, error) {
	rows, err := s.db.Query(`SELECT l.name FROM issue_label il JOIN label l ON l.id=il.label_id WHERE il.issue_id=? ORDER BY l.name`, issueID)
	if err != nil {
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	defer rows.Close()
	var out []string
	for rows.Next() {
		var n string
		if err := rows.Scan(&n); err != nil {
			return nil, fmt.Errorf("%w: %v", ErrInternal, err)
		}
		out = append(out, n)
	}
	return out, rows.Err()
}
