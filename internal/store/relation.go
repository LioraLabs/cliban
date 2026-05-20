package store

import (
	"database/sql"
	"errors"
	"fmt"

	"github.com/alex/cliban/internal/domain"
)

// RelationKind enumerates the supported issue↔issue relation types.
type RelationKind string

const (
	RelBlocks    RelationKind = "blocks"     // from blocks to
	RelRelatedTo RelationKind = "related_to" // symmetric reference
)

// AddRelation creates a relation from one issue to another.
// `blocked_by` is expressed as `blocks` in the opposite direction.
func (s *Store) AddRelation(from, to domain.IssueKey, kind RelationKind) error {
	if kind != RelBlocks && kind != RelRelatedTo {
		return fmt.Errorf("%w: invalid relation kind %q", ErrValidation, kind)
	}
	if from.Project == to.Project && from.Seq == to.Seq {
		return fmt.Errorf("%w: issue cannot relate to itself", ErrValidation)
	}
	fromID, err := s.issueIDByKey(from)
	if err != nil {
		return err
	}
	toID, err := s.issueIDByKey(to)
	if err != nil {
		return err
	}
	if _, err := s.db.Exec(`INSERT OR IGNORE INTO issue_relation(from_issue_id,to_issue_id,type,created_at) VALUES(?,?,?,?)`,
		fromID, toID, string(kind), s.nowISO()); err != nil {
		return fmt.Errorf("%w: %v", ErrInternal, err)
	}
	// related_to is symmetric — also insert the reverse if missing.
	if kind == RelRelatedTo {
		if _, err := s.db.Exec(`INSERT OR IGNORE INTO issue_relation(from_issue_id,to_issue_id,type,created_at) VALUES(?,?,?,?)`,
			toID, fromID, string(kind), s.nowISO()); err != nil {
			return fmt.Errorf("%w: %v", ErrInternal, err)
		}
	}
	return nil
}

// RemoveRelation removes a relation from one issue to another (the symmetric
// edge of related_to is also removed).
func (s *Store) RemoveRelation(from, to domain.IssueKey, kind RelationKind) error {
	fromID, err := s.issueIDByKey(from)
	if err != nil {
		return err
	}
	toID, err := s.issueIDByKey(to)
	if err != nil {
		return err
	}
	if _, err := s.db.Exec(`DELETE FROM issue_relation WHERE from_issue_id=? AND to_issue_id=? AND type=?`, fromID, toID, string(kind)); err != nil {
		return fmt.Errorf("%w: %v", ErrInternal, err)
	}
	if kind == RelRelatedTo {
		if _, err := s.db.Exec(`DELETE FROM issue_relation WHERE from_issue_id=? AND to_issue_id=? AND type=?`, toID, fromID, string(kind)); err != nil {
			return fmt.Errorf("%w: %v", ErrInternal, err)
		}
	}
	return nil
}

func (s *Store) issueIDByKey(k domain.IssueKey) (int64, error) {
	var id int64
	if err := s.db.QueryRow(`SELECT i.id FROM issue i JOIN project p ON p.id=i.project_id WHERE p.key=? AND i.seq=?`, k.Project, k.Seq).Scan(&id); err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return 0, fmt.Errorf("%w: issue %s", ErrNotFound, k)
		}
		return 0, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	return id, nil
}

// Relation describes one outgoing edge with the other issue's key.
type Relation struct {
	Kind   RelationKind
	Target domain.IssueKey
}

// RelationsForIssue returns the outgoing relations + the "blocked_by" reverse
// edges for an issue. The result is sorted by kind+target for stability.
func (s *Store) RelationsForIssue(issueID int64) ([]Relation, error) {
	// outgoing edges
	rows, err := s.db.Query(`
		SELECT r.type, p.key, i.seq
		FROM issue_relation r
		JOIN issue i ON i.id = r.to_issue_id
		JOIN project p ON p.id = i.project_id
		WHERE r.from_issue_id = ?
		ORDER BY r.type, p.key, i.seq
	`, issueID)
	if err != nil {
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	defer rows.Close()
	var out []Relation
	for rows.Next() {
		var kind, projKey string
		var seq int64
		if err := rows.Scan(&kind, &projKey, &seq); err != nil {
			return nil, fmt.Errorf("%w: %v", ErrInternal, err)
		}
		out = append(out, Relation{Kind: RelationKind(kind), Target: domain.IssueKey{Project: projKey, Seq: seq}})
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	// incoming "blocks" edges show up as "blocked_by"
	rows2, err := s.db.Query(`
		SELECT p.key, i.seq
		FROM issue_relation r
		JOIN issue i ON i.id = r.from_issue_id
		JOIN project p ON p.id = i.project_id
		WHERE r.to_issue_id = ? AND r.type = 'blocks'
		ORDER BY p.key, i.seq
	`, issueID)
	if err != nil {
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	defer rows2.Close()
	for rows2.Next() {
		var projKey string
		var seq int64
		if err := rows2.Scan(&projKey, &seq); err != nil {
			return nil, fmt.Errorf("%w: %v", ErrInternal, err)
		}
		out = append(out, Relation{Kind: "blocked_by", Target: domain.IssueKey{Project: projKey, Seq: seq}})
	}
	return out, rows2.Err()
}

// ListBlockedIssues returns every non-archived issue that has at least one
// open (non-done, non-archived) blocker. When projectKey != "", restricts to
// that project.
func (s *Store) ListBlockedIssues(projectKey string) ([]*domain.Issue, error) {
	q := `
		SELECT DISTINCT ` + issueSelectColsJoined + `
		FROM issue i
		JOIN project p ON p.id = i.project_id
		JOIN issue_relation r ON r.to_issue_id = i.id AND r.type = 'blocks'
		JOIN issue blocker ON blocker.id = r.from_issue_id
		WHERE i.archived = 0
		  AND blocker.archived = 0
		  AND blocker.status != 'done'`
	args := []any{}
	if projectKey != "" {
		q += ` AND p.key = ?`
		args = append(args, projectKey)
	}
	q += ` ORDER BY p.key, i.status, i.position`
	rows, err := s.db.Query(q, args...)
	if err != nil {
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	defer rows.Close()
	var out []*domain.Issue
	for rows.Next() {
		issue, err := scanIssue(rows)
		if err != nil {
			return nil, err
		}
		out = append(out, issue)
	}
	return out, rows.Err()
}
