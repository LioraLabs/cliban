package store

import (
	"errors"
	"testing"

	"github.com/alex/cliban/internal/domain"
)

func mustProj(t *testing.T, s *Store) *domain.Project {
	t.Helper()
	p, err := s.CreateProject("CLI", "Cliban", "")
	if err != nil {
		t.Fatal(err)
	}
	return p
}

func TestCreateIssueAssignsSequentialSeq(t *testing.T) {
	s := newTestStore(t)
	mustProj(t, s)
	i1, err := s.CreateIssue(CreateIssueParams{ProjectKey: "CLI", Title: "first"})
	if err != nil {
		t.Fatalf("create #1: %v", err)
	}
	if i1.Seq != 1 {
		t.Errorf("seq #1 = %d want 1", i1.Seq)
	}
	i2, err := s.CreateIssue(CreateIssueParams{ProjectKey: "CLI", Title: "second"})
	if err != nil {
		t.Fatalf("create #2: %v", err)
	}
	if i2.Seq != 2 {
		t.Errorf("seq #2 = %d want 2", i2.Seq)
	}
	p, _ := s.GetProjectByKey("CLI")
	if p.IssueSeq != 2 {
		t.Errorf("project.IssueSeq = %d want 2", p.IssueSeq)
	}
}

func TestCreateIssueDefaults(t *testing.T) {
	s := newTestStore(t)
	mustProj(t, s)
	i, _ := s.CreateIssue(CreateIssueParams{ProjectKey: "CLI", Title: "x"})
	if i.Status != domain.StatusBacklog {
		t.Errorf("default status = %q want backlog", i.Status)
	}
	if i.Priority != domain.PriorityNone {
		t.Errorf("default priority = %q want none", i.Priority)
	}
	if i.Position == 0 {
		t.Error("default position should be > 0")
	}
}

func TestCreateIssueRequiresTitle(t *testing.T) {
	s := newTestStore(t)
	mustProj(t, s)
	_, err := s.CreateIssue(CreateIssueParams{ProjectKey: "CLI", Title: ""})
	if !errors.Is(err, ErrValidation) {
		t.Errorf("want ErrValidation, got %v", err)
	}
}

func TestCreateIssueUnknownProject(t *testing.T) {
	s := newTestStore(t)
	_, err := s.CreateIssue(CreateIssueParams{ProjectKey: "NOPE", Title: "x"})
	if !errors.Is(err, ErrNotFound) {
		t.Errorf("want ErrNotFound, got %v", err)
	}
}

func TestCreateIssueWithMilestone(t *testing.T) {
	s := newTestStore(t)
	mustProj(t, s)
	_, _ = s.CreateMilestone("CLI", "v0.1", "", nil)
	i, err := s.CreateIssue(CreateIssueParams{
		ProjectKey:    "CLI",
		Title:         "x",
		MilestoneName: "v0.1",
	})
	if err != nil {
		t.Fatalf("CreateIssue: %v", err)
	}
	if i.MilestoneID == nil {
		t.Error("MilestoneID is nil")
	}
}

func TestGetIssueByKey(t *testing.T) {
	s := newTestStore(t)
	mustProj(t, s)
	created, _ := s.CreateIssue(CreateIssueParams{ProjectKey: "CLI", Title: "x"})
	got, err := s.GetIssueByKey(domain.IssueKey{Project: "CLI", Seq: created.Seq})
	if err != nil {
		t.Fatalf("GetIssueByKey: %v", err)
	}
	if got.ID != created.ID {
		t.Errorf("ID mismatch")
	}
	if _, err := s.GetIssueByKey(domain.IssueKey{Project: "CLI", Seq: 999}); !errors.Is(err, ErrNotFound) {
		t.Errorf("want ErrNotFound, got %v", err)
	}
}

func TestListIssues(t *testing.T) {
	s := newTestStore(t)
	mustProj(t, s)
	_, _ = s.CreateIssue(CreateIssueParams{ProjectKey: "CLI", Title: "a"})
	_, _ = s.CreateIssue(CreateIssueParams{ProjectKey: "CLI", Title: "b", Status: domain.StatusInProgress})
	all, _ := s.ListIssues(ListIssuesFilter{})
	if len(all) != 2 {
		t.Errorf("all len=%d want 2", len(all))
	}
	inProg, _ := s.ListIssues(ListIssuesFilter{Status: domain.StatusInProgress})
	if len(inProg) != 1 {
		t.Errorf("in-progress len=%d want 1", len(inProg))
	}
}
