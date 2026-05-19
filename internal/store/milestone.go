package store

import (
	"database/sql"
	"errors"
	"fmt"
	"time"

	"github.com/alex/cliban/internal/domain"
)

const milestoneSelectCols = `id,project_id,name,description,target_date,status,created_at,updated_at`

type UpdateMilestoneParams struct {
	NewName     *string
	Description *string
	TargetDate  *time.Time
	ClearTarget bool
	Status      *string
}

func (s *Store) CreateMilestone(projectKey, name, description string, target *time.Time) (*domain.Milestone, error) {
	if name == "" {
		return nil, fmt.Errorf("%w: milestone name required", ErrValidation)
	}
	proj, err := s.GetProjectByKey(projectKey)
	if err != nil {
		return nil, err
	}
	now := s.nowISO()
	var targetStr any
	if target != nil {
		targetStr = target.UTC().Format("2006-01-02")
	}
	res, err := s.db.Exec(`INSERT INTO milestone(project_id,name,description,target_date,status,created_at,updated_at) VALUES(?,?,?,?,?,?,?)`,
		proj.ID, name, description, targetStr, "open", now, now)
	if err != nil {
		if isUniqueErr(err) {
			return nil, fmt.Errorf("%w: milestone %q already exists in %s", ErrConflict, name, projectKey)
		}
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	id, _ := res.LastInsertId()
	return s.getMilestoneByID(id)
}

func (s *Store) GetMilestone(projectKey, name string) (*domain.Milestone, error) {
	proj, err := s.GetProjectByKey(projectKey)
	if err != nil {
		return nil, err
	}
	row := s.db.QueryRow(`SELECT `+milestoneSelectCols+` FROM milestone WHERE project_id=? AND name=?`, proj.ID, name)
	return scanMilestone(row)
}

func (s *Store) getMilestoneByID(id int64) (*domain.Milestone, error) {
	row := s.db.QueryRow(`SELECT `+milestoneSelectCols+` FROM milestone WHERE id=?`, id)
	return scanMilestone(row)
}

func scanMilestone(r interface{ Scan(...any) error }) (*domain.Milestone, error) {
	var m domain.Milestone
	var target sql.NullString
	var status, created, updated string
	if err := r.Scan(&m.ID, &m.ProjectID, &m.Name, &m.Description, &target, &status, &created, &updated); err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return nil, ErrNotFound
		}
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	if target.Valid {
		t, _ := time.Parse("2006-01-02", target.String)
		m.TargetDate = &t
	}
	m.Status = domain.MilestoneStatus(status)
	m.CreatedAt, _ = time.Parse(time.RFC3339Nano, created)
	m.UpdatedAt, _ = time.Parse(time.RFC3339Nano, updated)
	return &m, nil
}

func (s *Store) ListMilestones(projectKey, statusFilter string) ([]*domain.Milestone, error) {
	proj, err := s.GetProjectByKey(projectKey)
	if err != nil {
		return nil, err
	}
	q := `SELECT ` + milestoneSelectCols + ` FROM milestone WHERE project_id=?`
	args := []any{proj.ID}
	if statusFilter != "" {
		q += ` AND status=?`
		args = append(args, statusFilter)
	}
	q += ` ORDER BY name`
	rows, err := s.db.Query(q, args...)
	if err != nil {
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	defer rows.Close()
	var out []*domain.Milestone
	for rows.Next() {
		m, err := scanMilestone(rows)
		if err != nil {
			return nil, err
		}
		out = append(out, m)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("%w: %v", ErrInternal, err)
	}
	return out, nil
}

func (s *Store) UpdateMilestone(projectKey, name string, p UpdateMilestoneParams) error {
	cur, err := s.GetMilestone(projectKey, name)
	if err != nil {
		return err
	}
	newName := cur.Name
	if p.NewName != nil {
		if *p.NewName == "" {
			return fmt.Errorf("%w: name cannot be empty", ErrValidation)
		}
		newName = *p.NewName
	}
	desc := cur.Description
	if p.Description != nil {
		desc = *p.Description
	}
	status := cur.Status
	if p.Status != nil {
		switch *p.Status {
		case "open", "completed", "cancelled":
			status = domain.MilestoneStatus(*p.Status)
		default:
			return fmt.Errorf("%w: invalid milestone status %q", ErrValidation, *p.Status)
		}
	}
	var targetStr any
	switch {
	case p.ClearTarget:
		targetStr = nil
	case p.TargetDate != nil:
		targetStr = p.TargetDate.UTC().Format("2006-01-02")
	case cur.TargetDate != nil:
		targetStr = cur.TargetDate.UTC().Format("2006-01-02")
	}
	_, err = s.db.Exec(`UPDATE milestone SET name=?, description=?, target_date=?, status=?, updated_at=? WHERE id=?`,
		newName, desc, targetStr, string(status), s.nowISO(), cur.ID)
	if err != nil {
		if isUniqueErr(err) {
			return fmt.Errorf("%w: milestone %q already exists", ErrConflict, newName)
		}
		return fmt.Errorf("%w: %v", ErrInternal, err)
	}
	return nil
}

func (s *Store) DeleteMilestone(projectKey, name string) error {
	cur, err := s.GetMilestone(projectKey, name)
	if err != nil {
		return err
	}
	_, err = s.db.Exec(`DELETE FROM milestone WHERE id=?`, cur.ID)
	if err != nil {
		return fmt.Errorf("%w: %v", ErrInternal, err)
	}
	return nil
}
