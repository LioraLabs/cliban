package tui

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestApplyMilestoneBufferCreatesMilestone(t *testing.T) {
	s := newStore(t)
	_, _ = s.CreateProject("CLI", "Cliban", "")

	dir := t.TempDir()
	path := filepath.Join(dir, "milestone.md")
	content := "# header line\n" +
		"---\n" +
		"name:   v0.1\n" +
		"target: 2026-06-01\n" +
		"status: open\n" +
		"---\n" +
		"First release.\n"
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}

	if err := applyMilestoneBuffer(s, "CLI", path); err != nil {
		t.Fatalf("applyMilestoneBuffer: %v", err)
	}

	m, err := s.GetMilestone("CLI", "v0.1")
	if err != nil {
		t.Fatalf("GetMilestone: %v", err)
	}
	if m.Name != "v0.1" {
		t.Errorf("Name=%q want v0.1", m.Name)
	}
	if m.TargetDate == nil || m.TargetDate.Format("2006-01-02") != "2026-06-01" {
		t.Errorf("TargetDate=%v want 2026-06-01", m.TargetDate)
	}
	if m.Description != "First release.\n" {
		t.Errorf("Description=%q want %q", m.Description, "First release.\n")
	}
}

func TestApplyMilestoneEditBufferUpdatesExisting(t *testing.T) {
	s := newStore(t)
	_, _ = s.CreateProject("CLI", "Cliban", "")
	target := mustDate(t, "2026-06-01")
	_, _ = s.CreateMilestone("CLI", "v0.1", "old text", &target)

	dir := t.TempDir()
	path := filepath.Join(dir, "milestone.md")
	content := "# header\n" +
		"---\n" +
		"name:   v0.1\n" +
		"target: 2026-07-15\n" +
		"status: completed\n" +
		"---\n" +
		"New text.\n"
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}

	if _, err := applyMilestoneEditBuffer(s, "CLI", "v0.1", path); err != nil {
		t.Fatalf("applyMilestoneEditBuffer: %v", err)
	}

	m, err := s.GetMilestone("CLI", "v0.1")
	if err != nil {
		t.Fatalf("GetMilestone: %v", err)
	}
	if m.Description != "New text.\n" {
		t.Errorf("Description=%q want %q", m.Description, "New text.\n")
	}
	if string(m.Status) != "completed" {
		t.Errorf("Status=%q want completed", m.Status)
	}
	if m.TargetDate == nil || m.TargetDate.Format("2006-01-02") != "2026-07-15" {
		t.Errorf("TargetDate=%v want 2026-07-15", m.TargetDate)
	}
}

func TestApplyMilestoneEditBufferRenames(t *testing.T) {
	s := newStore(t)
	_, _ = s.CreateProject("CLI", "Cliban", "")
	_, _ = s.CreateMilestone("CLI", "v0.1", "", nil)

	dir := t.TempDir()
	path := filepath.Join(dir, "milestone.md")
	_ = os.WriteFile(path, []byte("---\nname: v1.0\nstatus: open\ntarget:\n---\n"), 0o644)

	newName, err := applyMilestoneEditBuffer(s, "CLI", "v0.1", path)
	if err != nil {
		t.Fatalf("applyMilestoneEditBuffer: %v", err)
	}
	if newName != "v1.0" {
		t.Errorf("returned name=%q want v1.0", newName)
	}
	if _, err := s.GetMilestone("CLI", "v1.0"); err != nil {
		t.Errorf("renamed milestone not found: %v", err)
	}
	if _, err := s.GetMilestone("CLI", "v0.1"); err == nil {
		t.Error("old milestone name still exists after rename")
	}
}

func TestApplyMilestoneEditBufferClearsTarget(t *testing.T) {
	s := newStore(t)
	_, _ = s.CreateProject("CLI", "Cliban", "")
	target := mustDate(t, "2026-06-01")
	_, _ = s.CreateMilestone("CLI", "v0.1", "", &target)

	dir := t.TempDir()
	path := filepath.Join(dir, "milestone.md")
	_ = os.WriteFile(path, []byte("---\nname: v0.1\nstatus: open\ntarget:\n---\n"), 0o644)

	if _, err := applyMilestoneEditBuffer(s, "CLI", "v0.1", path); err != nil {
		t.Fatalf("applyMilestoneEditBuffer: %v", err)
	}
	m, err := s.GetMilestone("CLI", "v0.1")
	if err != nil {
		t.Fatal(err)
	}
	if m.TargetDate != nil {
		t.Errorf("TargetDate=%v want nil (cleared)", m.TargetDate)
	}
}

func mustDate(t *testing.T, s string) time.Time {
	t.Helper()
	d, err := time.Parse("2006-01-02", s)
	if err != nil {
		t.Fatal(err)
	}
	return d
}

func TestApplyMilestoneBufferInvalidTarget(t *testing.T) {
	s := newStore(t)
	_, _ = s.CreateProject("CLI", "Cliban", "")
	dir := t.TempDir()
	path := filepath.Join(dir, "milestone.md")
	content := "---\nname: v0.1\ntarget: not-a-date\nstatus: open\n---\n"
	_ = os.WriteFile(path, []byte(content), 0o644)
	if err := applyMilestoneBuffer(s, "CLI", path); err == nil {
		t.Error("expected error on bad target date")
	}
}
