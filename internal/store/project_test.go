package store

import (
	"errors"
	"testing"

	"github.com/alex/cliban/internal/domain"
)

func TestCreateProject(t *testing.T) {
	s := newTestStore(t)
	p, err := s.CreateProject("CLI", "Cliban", "the kanban")
	if err != nil {
		t.Fatalf("CreateProject: %v", err)
	}
	if p.Key != "CLI" || p.Name != "Cliban" || p.IssueSeq != 0 {
		t.Errorf("unexpected project: %+v", p)
	}
	if p.CreatedAt.IsZero() {
		t.Error("CreatedAt zero")
	}
}

func TestCreateProjectDuplicateKey(t *testing.T) {
	s := newTestStore(t)
	if _, err := s.CreateProject("CLI", "A", ""); err != nil {
		t.Fatal(err)
	}
	_, err := s.CreateProject("CLI", "B", "")
	if !errors.Is(err, ErrConflict) {
		t.Errorf("want ErrConflict, got %v", err)
	}
}

func TestCreateProjectValidatesKey(t *testing.T) {
	s := newTestStore(t)
	cases := []string{"", "lower", "with space", "WITH-DASH", "123ABC"}
	for _, k := range cases {
		if _, err := s.CreateProject(k, "x", ""); !errors.Is(err, ErrValidation) {
			t.Errorf("CreateProject(%q) want ErrValidation, got %v", k, err)
		}
	}
}

func TestGetProject(t *testing.T) {
	s := newTestStore(t)
	created, _ := s.CreateProject("CLI", "Cliban", "")
	got, err := s.GetProjectByKey("CLI")
	if err != nil {
		t.Fatalf("GetProjectByKey: %v", err)
	}
	if got.ID != created.ID {
		t.Errorf("ID mismatch: %d vs %d", got.ID, created.ID)
	}
	if _, err := s.GetProjectByKey("NOPE"); !errors.Is(err, ErrNotFound) {
		t.Errorf("want ErrNotFound, got %v", err)
	}
}

func TestListProjects(t *testing.T) {
	s := newTestStore(t)
	_, _ = s.CreateProject("AAA", "a", "")
	_, _ = s.CreateProject("BBB", "b", "")
	ps, err := s.ListProjects(false)
	if err != nil {
		t.Fatal(err)
	}
	if len(ps) != 2 {
		t.Errorf("len=%d want 2", len(ps))
	}
	if err := s.SetProjectArchived("AAA", true); err != nil {
		t.Fatal(err)
	}
	ps, _ = s.ListProjects(false)
	if len(ps) != 1 {
		t.Errorf("after archive, len=%d want 1", len(ps))
	}
	ps, _ = s.ListProjects(true)
	if len(ps) != 2 {
		t.Errorf("with archived, len=%d want 2", len(ps))
	}
}

func TestUpdateProject(t *testing.T) {
	s := newTestStore(t)
	_, _ = s.CreateProject("CLI", "Cliban", "old")
	if err := s.UpdateProject("CLI", "Cliban v2", "new"); err != nil {
		t.Fatal(err)
	}
	p, _ := s.GetProjectByKey("CLI")
	if p.Name != "Cliban v2" || p.Description != "new" {
		t.Errorf("unexpected: %+v", p)
	}
}

func TestDeleteProject(t *testing.T) {
	s := newTestStore(t)
	_, _ = s.CreateProject("CLI", "Cliban", "")
	if err := s.DeleteProject("CLI"); err != nil {
		t.Fatal(err)
	}
	if _, err := s.GetProjectByKey("CLI"); !errors.Is(err, ErrNotFound) {
		t.Errorf("expected ErrNotFound after delete, got %v", err)
	}
}

var _ = domain.Project{}
