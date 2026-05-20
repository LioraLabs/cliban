package tui

import (
	"os"
	"path/filepath"
	"testing"
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
