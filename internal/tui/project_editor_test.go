package tui

import (
	"os"
	"path/filepath"
	"testing"
)

func TestApplyProjectBufferUpdatesProject(t *testing.T) {
	s := newStore(t)
	_, _ = s.CreateProject("CLI", "Cliban", "old description")

	dir := t.TempDir()
	path := filepath.Join(dir, "project.md")
	content := "# header line\n" +
		"---\n" +
		"name: Cliban CLI\n" +
		"---\n" +
		"New description.\n"
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}

	if err := applyProjectBuffer(s, "CLI", path); err != nil {
		t.Fatalf("applyProjectBuffer: %v", err)
	}

	p, err := s.GetProjectByKey("CLI")
	if err != nil {
		t.Fatalf("GetProjectByKey: %v", err)
	}
	if p.Name != "Cliban CLI" {
		t.Errorf("Name=%q want %q", p.Name, "Cliban CLI")
	}
	if p.Description != "New description.\n" {
		t.Errorf("Description=%q want %q", p.Description, "New description.\n")
	}
}

func TestApplyProjectBufferEmptyNameFails(t *testing.T) {
	s := newStore(t)
	_, _ = s.CreateProject("CLI", "Cliban", "")
	dir := t.TempDir()
	path := filepath.Join(dir, "project.md")
	_ = os.WriteFile(path, []byte("---\nname:\n---\n"), 0o644)
	if err := applyProjectBuffer(s, "CLI", path); err == nil {
		t.Error("expected error when name is empty")
	}
}
