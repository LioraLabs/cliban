package store

import (
	"errors"
	"testing"
	"time"
)

func TestCreateMilestone(t *testing.T) {
	s := newTestStore(t)
	_, _ = s.CreateProject("CLI", "Cliban", "")
	target := time.Date(2026, 6, 1, 0, 0, 0, 0, time.UTC)
	m, err := s.CreateMilestone("CLI", "v0.1", "first release", &target)
	if err != nil {
		t.Fatalf("CreateMilestone: %v", err)
	}
	if m.Name != "v0.1" || m.Status != "open" {
		t.Errorf("unexpected: %+v", m)
	}
	if m.TargetDate == nil || !m.TargetDate.Equal(target) {
		t.Errorf("target mismatch: %v", m.TargetDate)
	}
}

func TestCreateMilestoneUnknownProject(t *testing.T) {
	s := newTestStore(t)
	_, err := s.CreateMilestone("NOPE", "x", "", nil)
	if !errors.Is(err, ErrNotFound) {
		t.Errorf("want ErrNotFound, got %v", err)
	}
}

func TestCreateMilestoneDuplicateName(t *testing.T) {
	s := newTestStore(t)
	_, _ = s.CreateProject("CLI", "x", "")
	if _, err := s.CreateMilestone("CLI", "v0.1", "", nil); err != nil {
		t.Fatal(err)
	}
	_, err := s.CreateMilestone("CLI", "v0.1", "", nil)
	if !errors.Is(err, ErrConflict) {
		t.Errorf("want ErrConflict, got %v", err)
	}
}

func TestListMilestones(t *testing.T) {
	s := newTestStore(t)
	_, _ = s.CreateProject("CLI", "x", "")
	_, _ = s.CreateMilestone("CLI", "v0.1", "", nil)
	_, _ = s.CreateMilestone("CLI", "v0.2", "", nil)
	ms, err := s.ListMilestones("CLI", "")
	if err != nil {
		t.Fatal(err)
	}
	if len(ms) != 2 {
		t.Errorf("len=%d want 2", len(ms))
	}
}

func TestUpdateMilestone(t *testing.T) {
	s := newTestStore(t)
	_, _ = s.CreateProject("CLI", "x", "")
	_, _ = s.CreateMilestone("CLI", "v0.1", "", nil)
	target := time.Date(2026, 12, 1, 0, 0, 0, 0, time.UTC)
	if err := s.UpdateMilestone("CLI", "v0.1", UpdateMilestoneParams{
		NewName:     ptr("v0.2"),
		Description: ptr("renamed"),
		TargetDate:  &target,
		Status:      ptr("completed"),
	}); err != nil {
		t.Fatalf("UpdateMilestone: %v", err)
	}
	m, _ := s.GetMilestone("CLI", "v0.2")
	if m.Description != "renamed" || string(m.Status) != "completed" {
		t.Errorf("unexpected: %+v", m)
	}
}

func TestDeleteMilestone(t *testing.T) {
	s := newTestStore(t)
	_, _ = s.CreateProject("CLI", "x", "")
	_, _ = s.CreateMilestone("CLI", "v0.1", "", nil)
	if err := s.DeleteMilestone("CLI", "v0.1"); err != nil {
		t.Fatal(err)
	}
	if _, err := s.GetMilestone("CLI", "v0.1"); !errors.Is(err, ErrNotFound) {
		t.Errorf("want ErrNotFound, got %v", err)
	}
}

func ptr[T any](v T) *T { return &v }
