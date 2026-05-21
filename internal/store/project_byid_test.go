package store

import "testing"

func TestProjectsByID(t *testing.T) {
	s := newTestStore(t)
	p1, err := s.CreateProject("AAA", "A", "")
	if err != nil {
		t.Fatalf("CreateProject AAA: %v", err)
	}
	p2, err := s.CreateProject("BBB", "B", "")
	if err != nil {
		t.Fatalf("CreateProject BBB: %v", err)
	}

	m, err := s.ProjectsByID()
	if err != nil {
		t.Fatalf("ProjectsByID: %v", err)
	}
	if got := m[p1.ID]; got != "AAA" {
		t.Fatalf("m[%d] = %q want AAA", p1.ID, got)
	}
	if got := m[p2.ID]; got != "BBB" {
		t.Fatalf("m[%d] = %q want BBB", p2.ID, got)
	}
	if len(m) != 2 {
		t.Fatalf("want 2 entries, got %d: %v", len(m), m)
	}
}

func TestProjectsByID_Empty(t *testing.T) {
	s := newTestStore(t)
	m, err := s.ProjectsByID()
	if err != nil {
		t.Fatalf("ProjectsByID: %v", err)
	}
	if len(m) != 0 {
		t.Fatalf("empty DB must yield empty map, got %v", m)
	}
}
