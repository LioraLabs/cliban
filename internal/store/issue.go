package store

import (
	"database/sql"
	"errors"
	"fmt"
	"strings"
	"time"

	"github.com/alex/cliban/internal/descmd"
	"github.com/alex/cliban/internal/domain"
)

const issueSelectCols = `id,project_id,milestone_id,parent_id,seq,title,description,status,priority,position,archived,due_date,created_at,updated_at,completed_at`

const issueSelectColsJoined = `i.id,i.project_id,i.milestone_id,i.parent_id,i.seq,i.title,i.description,i.status,i.priority,i.position,i.archived,i.due_date,i.created_at,i.updated_at,i.completed_at`

type CreateIssueParams struct {
	ProjectKey    string
	Title         string
	Description   string
	Status        domain.Status
	Priority      domain.Priority
	MilestoneName string
	ParentKey     *domain.IssueKey
	DueDate       *time.Time
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
	var dueDate any
	if p.DueDate != nil {
		dueDate = p.DueDate.UTC().Format("2006-01-02")
	}
	res, err := tx.Exec(`INSERT INTO issue(project_id,milestone_id,parent_id,seq,title,description,status,priority,position,due_date,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)`,
		projID, milestoneID, parentID, newSeq, p.Title, p.Description, string(status), string(priority), pos, dueDate, now, now)
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
	var due, completed sql.NullString
	if err := r.Scan(&i.ID, &i.ProjectID, &milestone, &parent, &i.Seq, &i.Title, &i.Description, &status, &priority, &i.Position, &archived, &due, &created, &updated, &completed); err != nil {
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
	if due.Valid {
		t, _ := time.Parse("2006-01-02", due.String)
		i.DueDate = &t
	}
	i.CreatedAt, _ = time.Parse(time.RFC3339Nano, created)
	i.UpdatedAt, _ = time.Parse(time.RFC3339Nano, updated)
	if completed.Valid {
		t, _ := time.Parse(time.RFC3339Nano, completed.String)
		i.CompletedAt = &t
	}
	return &i, nil
}

type ListIssuesFilter struct {
	// ProjectKey is the legacy single-project filter; it is ignored when
	// Projects is non-empty.
	ProjectKey string
	// Projects, when non-empty, restricts results to issues whose project
	// key is in the set, OR'd via an IN(...) clause.
	Projects        []string
	Status          domain.Status
	Priority        domain.Priority
	MilestoneName   string
	ParentKey       *domain.IssueKey
	NoSubs          bool
	IncludeArchived bool
	// LabelNames filters to issues that have ALL of the given label names.
	LabelNames   []string
	UpdatedSince *time.Time // optional; if set, only issues with updated_at >= this UTC time
}

func (s *Store) ListIssues(f ListIssuesFilter) ([]*domain.Issue, error) {
	q := `SELECT ` + issueSelectColsJoined + ` FROM issue i JOIN project p ON p.id = i.project_id`
	var args []any
	var conds []string
	switch {
	case len(f.Projects) > 0:
		placeholders := strings.Repeat("?,", len(f.Projects))
		placeholders = strings.TrimRight(placeholders, ",")
		conds = append(conds, "p.key IN ("+placeholders+")")
		for _, k := range f.Projects {
			args = append(args, k)
		}
	case f.ProjectKey != "":
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
	for _, lbl := range f.LabelNames {
		conds = append(conds, `i.id IN (
			SELECT il.issue_id FROM issue_label il
			JOIN label l ON l.id = il.label_id
			WHERE l.name = ?
		)`)
		args = append(args, lbl)
	}
	if f.UpdatedSince != nil {
		conds = append(conds, "i.updated_at >= ?")
		args = append(args, f.UpdatedSince.UTC().Format(time.RFC3339Nano))
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
	DueDate        *time.Time
	ClearDueDate   bool
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
	case p.ClearDueDate:
		set = append(set, "due_date=NULL")
	case p.DueDate != nil:
		set = append(set, "due_date=?")
		args = append(args, p.DueDate.UTC().Format("2006-01-02"))
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

// TickResult describes the outcome of a successful TickStep call.
type TickResult struct {
	Key       domain.IssueKey
	TaskN     int
	StepM     int
	Checked   bool
	UpdatedAt time.Time
}

// TickStep flips the M-th step of task N in the description of issue k from
// unchecked to checked. The mutation is wrapped in a single SQL transaction
// so concurrent ticks are serialized by SQLite. Returns ErrValidation if the
// description's ## Plan / Task N / Step M structure is malformed, the task
// or step is missing, or the step is already checked.
func (s *Store) TickStep(k domain.IssueKey, taskN, stepM int) (*TickResult, error) {
	tx, err := s.db.Begin()
	if err != nil {
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	defer tx.Rollback()

	var id int64
	var desc string
	if err := tx.QueryRow(`SELECT i.id, i.description FROM issue i JOIN project p ON p.id=i.project_id WHERE p.key=? AND i.seq=?`, k.Project, k.Seq).
		Scan(&id, &desc); err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return nil, ErrNotFound
		}
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	newDesc, err := descmd.TickStep(desc, taskN, stepM)
	if err != nil {
		return nil, fmt.Errorf("%w: %v", ErrValidation, err)
	}
	now := s.nowISO()
	if _, err := tx.Exec(`UPDATE issue SET description=?, updated_at=? WHERE id=?`, newDesc, now, id); err != nil {
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	if err := tx.Commit(); err != nil {
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	ts, _ := time.Parse(time.RFC3339Nano, now)
	return &TickResult{Key: k, TaskN: taskN, StepM: stepM, Checked: true, UpdatedAt: ts}, nil
}

type PromoteMode string

const (
	PromoteAsSubIssue PromoteMode = "sub-issue"
	PromoteAsRelated  PromoteMode = "related"
)

type PromoteParams struct {
	Parent domain.IssueKey
	TaskN  int
	StepM  int
	Title  string
	Mode   PromoteMode
}

type PromoteResult struct {
	NewKey domain.IssueKey
	Parent domain.IssueKey
	TaskN  int
	StepM  int
}

// PromoteStep creates a new issue (sub-issue or top-level w/ related_to) and
// rewrites the step line in the parent's plan to reference the new issue.
// All effects happen in a single transaction.
func (s *Store) PromoteStep(p PromoteParams) (*PromoteResult, error) {
	if p.Title == "" {
		return nil, fmt.Errorf("%w: --title required", ErrValidation)
	}
	switch p.Mode {
	case PromoteAsSubIssue, PromoteAsRelated:
	default:
		return nil, fmt.Errorf("%w: invalid --as %q (want sub-issue|related)", ErrValidation, p.Mode)
	}
	tx, err := s.db.Begin()
	if err != nil {
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	defer tx.Rollback()

	// 1. Read parent issue + project.
	var parentID, projID int64
	var parentDesc string
	var parentParent sql.NullInt64
	var issueSeq int64
	if err := tx.QueryRow(`SELECT i.id, i.project_id, i.description, i.parent_id, p.issue_seq FROM issue i JOIN project p ON p.id=i.project_id WHERE p.key=? AND i.seq=?`,
		p.Parent.Project, p.Parent.Seq).Scan(&parentID, &projID, &parentDesc, &parentParent, &issueSeq); err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return nil, ErrNotFound
		}
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	if p.Mode == PromoteAsSubIssue && parentParent.Valid {
		return nil, fmt.Errorf("%w: cannot promote as sub-issue of a sub-issue (would exceed depth 2)", ErrValidation)
	}

	// 2. Allocate new issue seq and insert.
	newSeq := issueSeq + 1
	var maxPos sql.NullFloat64
	_ = tx.QueryRow(`SELECT MAX(position) FROM issue WHERE project_id=? AND status=?`, projID, string(domain.StatusBacklog)).Scan(&maxPos)
	pos := 1000.0
	if maxPos.Valid {
		pos = maxPos.Float64 + 1000.0
	}
	now := s.nowISO()
	var subParent any
	if p.Mode == PromoteAsSubIssue {
		subParent = parentID
	}
	res, err := tx.Exec(`INSERT INTO issue(project_id,milestone_id,parent_id,seq,title,description,status,priority,position,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)`,
		projID, nil, subParent, newSeq, p.Title, "", string(domain.StatusBacklog), string(domain.PriorityNone), pos, now, now)
	if err != nil {
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	newID, _ := res.LastInsertId()
	if _, err := tx.Exec(`UPDATE project SET issue_seq=?, updated_at=? WHERE id=?`, newSeq, now, projID); err != nil {
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}

	// 3. If related mode, insert a related_to relation in BOTH directions
	// (matches AddRelation's symmetric-edge convention so future reads from
	// either side see the relation).
	if p.Mode == PromoteAsRelated {
		if _, err := tx.Exec(`INSERT OR IGNORE INTO issue_relation(from_issue_id,to_issue_id,type,created_at) VALUES(?,?,?,?)`,
			newID, parentID, "related_to", now); err != nil {
			return nil, fmt.Errorf("%w: %v", ErrInternal, err)
		}
		if _, err := tx.Exec(`INSERT OR IGNORE INTO issue_relation(from_issue_id,to_issue_id,type,created_at) VALUES(?,?,?,?)`,
			parentID, newID, "related_to", now); err != nil {
			return nil, fmt.Errorf("%w: %v", ErrInternal, err)
		}
	}

	// 4. Rewrite the parent's step line.
	step, ok := findStepForRewrite(parentDesc, p.TaskN, p.StepM)
	if !ok {
		return nil, fmt.Errorf("%w: cannot find Task %d Step %d in parent description", ErrValidation, p.TaskN, p.StepM)
	}
	newKey := domain.IssueKey{Project: p.Parent.Project, Seq: newSeq}
	newLine := buildPromotedLine(step.Raw, p.Title, newKey)
	newDesc, err := descmd.RewriteStepLine(parentDesc, p.TaskN, p.StepM, newLine)
	if err != nil {
		return nil, fmt.Errorf("%w: %v", ErrValidation, err)
	}
	if _, err := tx.Exec(`UPDATE issue SET description=?, updated_at=? WHERE id=?`, newDesc, now, parentID); err != nil {
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}

	if err := tx.Commit(); err != nil {
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	return &PromoteResult{NewKey: newKey, Parent: p.Parent, TaskN: p.TaskN, StepM: p.StepM}, nil
}

// findStepForRewrite is a thin wrapper around descmd.FindSection + FindTask +
// FindStep used here to keep the existing Plan/Task/Step lookup contained.
func findStepForRewrite(desc string, taskN, stepM int) (descmd.Step, bool) {
	planStart, planEnd, ok := descmd.FindSection(desc, "Plan")
	if !ok {
		return descmd.Step{}, false
	}
	planBody := desc[planStart:planEnd]
	taskStart, taskEnd, ok := descmd.FindTask(planBody, taskN)
	if !ok {
		return descmd.Step{}, false
	}
	return descmd.FindStep(planBody[taskStart:taskEnd], stepM)
}

// buildPromotedLine produces the rewritten step line with the "→ KEY"
// suffix. If the original line already had a "→ ..." suffix, it's replaced.
func buildPromotedLine(originalLine, newTitle string, newKey domain.IssueKey) string {
	trimmed := strings.TrimRight(originalLine, "\n")
	if idx := strings.LastIndex(trimmed, " → "); idx >= 0 {
		trimmed = trimmed[:idx]
	}
	return fmt.Sprintf("%s → %s\n", trimmed, newKey.String())
}

type LogResult struct {
	Key       domain.IssueKey
	Entry     string
	Timestamp time.Time
}

// AppendActivityLog atomically appends a chronological entry to the
// ## Activity Log section of the issue's description, creating the section
// if absent.
func (s *Store) AppendActivityLog(k domain.IssueKey, message string) (*LogResult, error) {
	if message == "" {
		return nil, fmt.Errorf("%w: message required", ErrValidation)
	}
	tx, err := s.db.Begin()
	if err != nil {
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	defer tx.Rollback()

	var id int64
	var desc string
	if err := tx.QueryRow(`SELECT i.id, i.description FROM issue i JOIN project p ON p.id=i.project_id WHERE p.key=? AND i.seq=?`, k.Project, k.Seq).
		Scan(&id, &desc); err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return nil, ErrNotFound
		}
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	now := s.now().UTC()
	newDesc := descmd.AppendActivityLog(desc, message, now)
	if _, err := tx.Exec(`UPDATE issue SET description=?, updated_at=? WHERE id=?`, newDesc, now.Format(time.RFC3339Nano), id); err != nil {
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	if err := tx.Commit(); err != nil {
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	return &LogResult{Key: k, Entry: message, Timestamp: now}, nil
}
