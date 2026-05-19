package store

import (
	"database/sql"
	"errors"
	"fmt"
	"time"

	"github.com/alex/cliban/internal/domain"
)

const issueSelectCols = `id,project_id,milestone_id,parent_id,seq,title,description,status,priority,position,created_at,updated_at,completed_at`

const issueSelectColsJoined = `i.id,i.project_id,i.milestone_id,i.parent_id,i.seq,i.title,i.description,i.status,i.priority,i.position,i.created_at,i.updated_at,i.completed_at`

type CreateIssueParams struct {
	ProjectKey    string
	Title         string
	Description   string
	Status        domain.Status
	Priority      domain.Priority
	MilestoneName string
	ParentKey     *domain.IssueKey
}

func (s *Store) CreateIssue(p CreateIssueParams) (*domain.Issue, error) {
	if p.Title == "" {
		return nil, fmt.Errorf("%w: title required", ErrValidation)
	}
	status := p.Status
	if status == "" {
		status = domain.StatusBacklog
	} else if _, err := domain.ParseStatus(string(status)); err != nil {
		return nil, fmt.Errorf("%w: %v", ErrValidation, err)
	}
	priority := p.Priority
	if priority == "" {
		priority = domain.PriorityNone
	} else if _, err := domain.ParsePriority(string(priority)); err != nil {
		return nil, fmt.Errorf("%w: %v", ErrValidation, err)
	}

	tx, err := s.db.Begin()
	if err != nil {
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	defer tx.Rollback()

	var projID, issueSeq int64
	if err := tx.QueryRow(`SELECT id, issue_seq FROM project WHERE key=?`, p.ProjectKey).Scan(&projID, &issueSeq); err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return nil, ErrNotFound
		}
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	newSeq := issueSeq + 1

	var milestoneID *int64
	if p.MilestoneName != "" {
		var id int64
		if err := tx.QueryRow(`SELECT id FROM milestone WHERE project_id=? AND name=?`, projID, p.MilestoneName).Scan(&id); err != nil {
			if errors.Is(err, sql.ErrNoRows) {
				return nil, fmt.Errorf("%w: milestone %q in %s", ErrNotFound, p.MilestoneName, p.ProjectKey)
			}
			return nil, fmt.Errorf("%w: %v", ErrInternal, err)
		}
		milestoneID = &id
	}

	var parentID *int64
	if p.ParentKey != nil {
		if p.ParentKey.Project != p.ProjectKey {
			return nil, fmt.Errorf("%w: parent issue must be in same project (parent=%s, project=%s)", ErrValidation, p.ParentKey.Project, p.ProjectKey)
		}
		var id int64
		var existingParent sql.NullInt64
		if err := tx.QueryRow(`SELECT id, parent_id FROM issue WHERE project_id=? AND seq=?`, projID, p.ParentKey.Seq).Scan(&id, &existingParent); err != nil {
			if errors.Is(err, sql.ErrNoRows) {
				return nil, fmt.Errorf("%w: parent %s", ErrNotFound, p.ParentKey)
			}
			return nil, fmt.Errorf("%w: %v", ErrInternal, err)
		}
		if existingParent.Valid {
			return nil, fmt.Errorf("%w: sub-issue depth limited to 2 (parent %s is itself a sub-issue)", ErrValidation, p.ParentKey)
		}
		parentID = &id
	}

	var maxPos sql.NullFloat64
	if err := tx.QueryRow(`SELECT MAX(position) FROM issue WHERE project_id=? AND status=?`, projID, string(status)).Scan(&maxPos); err != nil && !errors.Is(err, sql.ErrNoRows) {
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	pos := 1000.0
	if maxPos.Valid {
		pos = maxPos.Float64 + 1000.0
	}

	now := s.nowISO()
	res, err := tx.Exec(`INSERT INTO issue(project_id,milestone_id,parent_id,seq,title,description,status,priority,position,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)`,
		projID, milestoneID, parentID, newSeq, p.Title, p.Description, string(status), string(priority), pos, now, now)
	if err != nil {
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	if _, err := tx.Exec(`UPDATE project SET issue_seq=?, updated_at=? WHERE id=?`, newSeq, now, projID); err != nil {
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	id, _ := res.LastInsertId()
	if err := tx.Commit(); err != nil {
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	return s.getIssueByID(id)
}

func (s *Store) GetIssueByKey(k domain.IssueKey) (*domain.Issue, error) {
	row := s.db.QueryRow(`SELECT `+issueSelectColsJoined+` FROM issue i JOIN project p ON p.id=i.project_id WHERE p.key=? AND i.seq=?`, k.Project, k.Seq)
	return scanIssue(row)
}

func (s *Store) getIssueByID(id int64) (*domain.Issue, error) {
	row := s.db.QueryRow(`SELECT `+issueSelectCols+` FROM issue WHERE id=?`, id)
	return scanIssue(row)
}

func scanIssue(r interface{ Scan(...any) error }) (*domain.Issue, error) {
	var i domain.Issue
	var milestone, parent sql.NullInt64
	var status, priority, created, updated string
	var completed sql.NullString
	if err := r.Scan(&i.ID, &i.ProjectID, &milestone, &parent, &i.Seq, &i.Title, &i.Description, &status, &priority, &i.Position, &created, &updated, &completed); err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return nil, ErrNotFound
		}
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	if milestone.Valid {
		v := milestone.Int64
		i.MilestoneID = &v
	}
	if parent.Valid {
		v := parent.Int64
		i.ParentID = &v
	}
	i.Status = domain.Status(status)
	i.Priority = domain.Priority(priority)
	i.CreatedAt, _ = time.Parse(time.RFC3339Nano, created)
	i.UpdatedAt, _ = time.Parse(time.RFC3339Nano, updated)
	if completed.Valid {
		t, _ := time.Parse(time.RFC3339Nano, completed.String)
		i.CompletedAt = &t
	}
	return &i, nil
}

type ListIssuesFilter struct {
	ProjectKey    string
	Status        domain.Status
	Priority      domain.Priority
	MilestoneName string
	ParentKey     *domain.IssueKey
	NoSubs        bool
}

func (s *Store) ListIssues(f ListIssuesFilter) ([]*domain.Issue, error) {
	q := `SELECT ` + issueSelectColsJoined + ` FROM issue i JOIN project p ON p.id = i.project_id`
	var args []any
	var conds []string
	if f.ProjectKey != "" {
		conds = append(conds, "p.key = ?")
		args = append(args, f.ProjectKey)
	}
	if f.Status != "" {
		conds = append(conds, "i.status = ?")
		args = append(args, string(f.Status))
	}
	if f.Priority != "" {
		conds = append(conds, "i.priority = ?")
		args = append(args, string(f.Priority))
	}
	if f.MilestoneName != "" {
		conds = append(conds, "i.milestone_id = (SELECT id FROM milestone WHERE project_id=p.id AND name=?)")
		args = append(args, f.MilestoneName)
	}
	if f.ParentKey != nil {
		conds = append(conds, "i.parent_id = (SELECT id FROM issue WHERE project_id=(SELECT id FROM project WHERE key=?) AND seq=?)")
		args = append(args, f.ParentKey.Project, f.ParentKey.Seq)
	}
	if f.NoSubs {
		conds = append(conds, "i.parent_id IS NULL")
	}
	for idx, c := range conds {
		if idx == 0 {
			q += " WHERE "
		} else {
			q += " AND "
		}
		q += c
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
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	return out, nil
}
