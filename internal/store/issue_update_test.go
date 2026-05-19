package store

import (
	"errors"
	"testing"

	"github.com/alex/cliban/internal/domain"
)

func TestUpdateIssue(t *testing.T) {
	s := newTestStore(t)
	mustProj(t, s)
	i, _ := s.CreateIssue(CreateIssueParams{ProjectKey: "CLI", Title: "x"})
	if err := s.UpdateIssue(domain.IssueKey{Project: "CLI", Seq: i.Seq}, UpdateIssueParams{
		Title:       ptr("new title"),
		Description: ptr("hello"),
		Priority:    ptr(domain.PriorityHigh),
	}); err != nil {
		t.Fatalf("UpdateIssue: %v", err)
	}
	got, _ := s.GetIssueByKey(domain.IssueKey{Project: "CLI", Seq: i.Seq})
	if got.Title != "new title" || got.Description != "hello" || got.Priority != domain.PriorityHigh {
		t.Errorf("unexpected: %+v", got)
	}
}

func TestMoveIssueSetsCompletedAt(t *testing.T) {
	s := newTestStore(t)
	mustProj(t, s)
	i, _ := s.CreateIssue(CreateIssueParams{ProjectKey: "CLI", Title: "x"})
	if err := s.MoveIssue(domain.IssueKey{Project: "CLI", Seq: i.Seq}, domain.StatusDone); err != nil {
		t.Fatal(err)
	}
	got, _ := s.GetIssueByKey(domain.IssueKey{Project: "CLI", Seq: i.Seq})
	if got.Status != domain.StatusDone {
		t.Errorf("status=%q want done", got.Status)
	}
	if got.CompletedAt == nil {
		t.Error("CompletedAt should be set when moving to done")
	}
	if err := s.MoveIssue(domain.IssueKey{Project: "CLI", Seq: i.Seq}, domain.StatusInProgress); err != nil {
		t.Fatal(err)
	}
	got, _ = s.GetIssueByKey(domain.IssueKey{Project: "CLI", Seq: i.Seq})
	if got.CompletedAt != nil {
		t.Error("CompletedAt should clear when moving out of done")
	}
}

func TestSubIssueDepthLimitEnforced(t *testing.T) {
	s := newTestStore(t)
	mustProj(t, s)
	root, _ := s.CreateIssue(CreateIssueParams{ProjectKey: "CLI", Title: "root"})
	child, err := s.CreateIssue(CreateIssueParams{ProjectKey: "CLI", Title: "child",
		ParentKey: &domain.IssueKey{Project: "CLI", Seq: root.Seq}})
	if err != nil {
		t.Fatalf("create child: %v", err)
	}
	_, err = s.CreateIssue(CreateIssueParams{ProjectKey: "CLI", Title: "grand",
		ParentKey: &domain.IssueKey{Project: "CLI", Seq: child.Seq}})
	if !errors.Is(err, ErrValidation) {
		t.Errorf("want ErrValidation for depth-3, got %v", err)
	}
}

func TestUpdateIssueCannotMakeSelfSubChild(t *testing.T) {
	s := newTestStore(t)
	mustProj(t, s)
	root, _ := s.CreateIssue(CreateIssueParams{ProjectKey: "CLI", Title: "root"})
	child, _ := s.CreateIssue(CreateIssueParams{ProjectKey: "CLI", Title: "child"})
	err := s.UpdateIssue(domain.IssueKey{Project: "CLI", Seq: child.Seq}, UpdateIssueParams{
		Parent: &domain.IssueKey{Project: "CLI", Seq: root.Seq},
	})
	if err != nil {
		t.Fatalf("UpdateIssue: %v", err)
	}
	err = s.UpdateIssue(domain.IssueKey{Project: "CLI", Seq: root.Seq}, UpdateIssueParams{
		Parent: &domain.IssueKey{Project: "CLI", Seq: child.Seq},
	})
	if !errors.Is(err, ErrValidation) {
		t.Errorf("want ErrValidation, got %v", err)
	}
}

func TestDeleteIssueCascadesSubIssues(t *testing.T) {
	s := newTestStore(t)
	mustProj(t, s)
	root, _ := s.CreateIssue(CreateIssueParams{ProjectKey: "CLI", Title: "root"})
	_, _ = s.CreateIssue(CreateIssueParams{ProjectKey: "CLI", Title: "child",
		ParentKey: &domain.IssueKey{Project: "CLI", Seq: root.Seq}})
	if err := s.DeleteIssue(domain.IssueKey{Project: "CLI", Seq: root.Seq}); err != nil {
		t.Fatal(err)
	}
	all, _ := s.ListIssues(ListIssuesFilter{ProjectKey: "CLI"})
	if len(all) != 0 {
		t.Errorf("after cascading delete, len=%d want 0", len(all))
	}
}
