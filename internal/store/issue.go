package store

import (
	"database/sql"
	"errors"
	"fmt"
	"strings"
	"time"

	"github.com/alex/cliban/internal/domain"
)

const issueSelectCols = `id,project_id,milestone_id,parent_id,seq,title,description,status,priority,position,archived,created_at,updated_at,completed_at`

const issueSelectColsJoined = `i.id,i.project_id,i.milestone_id,i.parent_id,i.seq,i.title,i.description,i.status,i.priority,i.position,i.archived,i.created_at,i.updated_at,i.completed_at`

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
	var archived int
	var completed sql.NullString
	if err := r.Scan(&i.ID, &i.ProjectID, &milestone, &parent, &i.Seq, &i.Title, &i.Description, &status, &priority, &i.Position, &archived, &created, &updated, &completed); err != nil {
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
	i.Archived = archived != 0
	i.CreatedAt, _ = time.Parse(time.RFC3339Nano, created)
	i.UpdatedAt, _ = time.Parse(time.RFC3339Nano, updated)
	if completed.Valid {
		t, _ := time.Parse(time.RFC3339Nano, completed.String)
		i.CompletedAt = &t
	}
	return &i, nil
}

type ListIssuesFilter struct {
	ProjectKey      string
	Status          domain.Status
	Priority        domain.Priority
	MilestoneName   string
	ParentKey       *domain.IssueKey
	NoSubs          bool
	IncludeArchived bool
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
	if !f.IncludeArchived {
		conds = append(conds, "i.archived = 0")
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

type UpdateIssueParams struct {
	Title          *string
	Description    *string
	Priority       *domain.Priority
	Milestone      *string
	ClearMilestone bool
	Parent         *domain.IssueKey
	ClearParent    bool
}

func (s *Store) UpdateIssue(k domain.IssueKey, p UpdateIssueParams) error {
	tx, err := s.db.Begin()
	if err != nil {
		return fmt.Errorf("%w: %v", ErrInternal, err)
	}
	defer tx.Rollback()

	var projID, id int64
	var curParent sql.NullInt64
	if err := tx.QueryRow(`SELECT i.id, i.project_id, i.parent_id FROM issue i JOIN project pr ON pr.id=i.project_id WHERE pr.key=? AND i.seq=?`, k.Project, k.Seq).
		Scan(&id, &projID, &curParent); err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return ErrNotFound
		}
		return fmt.Errorf("%w: %v", ErrInternal, err)
	}

	set := []string{}
	args := []any{}
	if p.Title != nil {
		if *p.Title == "" {
			return fmt.Errorf("%w: title cannot be empty", ErrValidation)
		}
		set = append(set, "title=?")
		args = append(args, *p.Title)
	}
	if p.Description != nil {
		set = append(set, "description=?")
		args = append(args, *p.Description)
	}
	if p.Priority != nil {
		if _, err := domain.ParsePriority(string(*p.Priority)); err != nil {
			return fmt.Errorf("%w: %v", ErrValidation, err)
		}
		set = append(set, "priority=?")
		args = append(args, string(*p.Priority))
	}
	switch {
	case p.ClearMilestone:
		set = append(set, "milestone_id=NULL")
	case p.Milestone != nil:
		var mid int64
		if err := tx.QueryRow(`SELECT id FROM milestone WHERE project_id=? AND name=?`, projID, *p.Milestone).Scan(&mid); err != nil {
			if errors.Is(err, sql.ErrNoRows) {
				return fmt.Errorf("%w: milestone %q", ErrNotFound, *p.Milestone)
			}
			return fmt.Errorf("%w: %v", ErrInternal, err)
		}
		set = append(set, "milestone_id=?")
		args = append(args, mid)
	}
	switch {
	case p.ClearParent:
		set = append(set, "parent_id=NULL")
	case p.Parent != nil:
		if p.Parent.Project != k.Project {
			return fmt.Errorf("%w: parent must be in same project", ErrValidation)
		}
		if p.Parent.Seq == k.Seq {
			return fmt.Errorf("%w: issue cannot be its own parent", ErrValidation)
		}
		var parentID int64
		var parentOfParent sql.NullInt64
		if err := tx.QueryRow(`SELECT id, parent_id FROM issue WHERE project_id=? AND seq=?`, projID, p.Parent.Seq).Scan(&parentID, &parentOfParent); err != nil {
			if errors.Is(err, sql.ErrNoRows) {
				return fmt.Errorf("%w: parent %s", ErrNotFound, p.Parent)
			}
			return fmt.Errorf("%w: %v", ErrInternal, err)
		}
		if parentOfParent.Valid {
			return fmt.Errorf("%w: depth-2 limit (target parent is itself a sub-issue)", ErrValidation)
		}
		var ownChildren int
		if err := tx.QueryRow(`SELECT COUNT(*) FROM issue WHERE parent_id=?`, id).Scan(&ownChildren); err != nil {
			return fmt.Errorf("%w: %v", ErrInternal, err)
		}
		if ownChildren > 0 {
			return fmt.Errorf("%w: issue has sub-issues; cannot also be made a sub-issue (would exceed depth 2)", ErrValidation)
		}
		set = append(set, "parent_id=?")
		args = append(args, parentID)
	}
	if len(set) == 0 {
		return nil
	}
	set = append(set, "updated_at=?")
	args = append(args, s.nowISO())
	args = append(args, id)
	query := "UPDATE issue SET " + strings.Join(set, ", ") + " WHERE id=?"
	if _, err := tx.Exec(query, args...); err != nil {
		return fmt.Errorf("%w: %v", ErrInternal, err)
	}
	return tx.Commit()
}

func (s *Store) MoveIssue(k domain.IssueKey, newStatus domain.Status) error {
	if _, err := domain.ParseStatus(string(newStatus)); err != nil {
		return fmt.Errorf("%w: %v", ErrValidation, err)
	}
	tx, err := s.db.Begin()
	if err != nil {
		return fmt.Errorf("%w: %v", ErrInternal, err)
	}
	defer tx.Rollback()

	var id, projID int64
	if err := tx.QueryRow(`SELECT i.id, i.project_id FROM issue i JOIN project p ON p.id=i.project_id WHERE p.key=? AND i.seq=?`, k.Project, k.Seq).Scan(&id, &projID); err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return ErrNotFound
		}
		return fmt.Errorf("%w: %v", ErrInternal, err)
	}
	var maxPos sql.NullFloat64
	_ = tx.QueryRow(`SELECT MAX(position) FROM issue WHERE project_id=? AND status=?`, projID, string(newStatus)).Scan(&maxPos)
	pos := 1000.0
	if maxPos.Valid {
		pos = maxPos.Float64 + 1000.0
	}
	now := s.nowISO()
	var completed any
	if newStatus == domain.StatusDone {
		completed = now
	} else {
		completed = nil
	}
	if _, err := tx.Exec(`UPDATE issue SET status=?, position=?, completed_at=?, updated_at=? WHERE id=?`,
		string(newStatus), pos, completed, now, id); err != nil {
		return fmt.Errorf("%w: %v", ErrInternal, err)
	}
	return tx.Commit()
}

func (s *Store) DeleteIssue(k domain.IssueKey) error {
	res, err := s.db.Exec(`DELETE FROM issue WHERE id = (SELECT i.id FROM issue i JOIN project p ON p.id=i.project_id WHERE p.key=? AND i.seq=?)`, k.Project, k.Seq)
	if err != nil {
		return fmt.Errorf("%w: %v", ErrInternal, err)
	}
	if n, _ := res.RowsAffected(); n == 0 {
		return ErrNotFound
	}
	return nil
}

func (s *Store) SetIssuePosition(k domain.IssueKey, newPos float64) error {
	res, err := s.db.Exec(`UPDATE issue SET position=?, updated_at=? WHERE id = (SELECT i.id FROM issue i JOIN project p ON p.id=i.project_id WHERE p.key=? AND i.seq=?)`,
		newPos, s.nowISO(), k.Project, k.Seq)
	if err != nil {
		return fmt.Errorf("%w: %v", ErrInternal, err)
	}
	if n, _ := res.RowsAffected(); n == 0 {
		return ErrNotFound
	}
	return nil
}

// GetIssueByID returns an issue by its internal ID.
func (s *Store) GetIssueByID(id int64) (*domain.Issue, error) {
	return s.getIssueByID(id)
}

func (s *Store) SetIssueArchived(k domain.IssueKey, archived bool) error {
	v := 0
	if archived {
		v = 1
	}
	res, err := s.db.Exec(`UPDATE issue SET archived=?, updated_at=? WHERE id = (SELECT i.id FROM issue i JOIN project p ON p.id=i.project_id WHERE p.key=? AND i.seq=?)`,
		v, s.nowISO(), k.Project, k.Seq)
	if err != nil {
		return fmt.Errorf("%w: %v", ErrInternal, err)
	}
	if n, _ := res.RowsAffected(); n == 0 {
		return ErrNotFound
	}
	return nil
}

// ArchiveDoneInProject archives every non-archived done issue in a project.
// Returns the number of issues archived.
func (s *Store) ArchiveDoneInProject(projectKey string) (int, error) {
	res, err := s.db.Exec(`UPDATE issue SET archived=1, updated_at=? WHERE archived=0 AND status='done' AND project_id = (SELECT id FROM project WHERE key=?)`,
		s.nowISO(), projectKey)
	if err != nil {
		return 0, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	n, _ := res.RowsAffected()
	return int(n), nil
}
