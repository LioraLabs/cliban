package tui

import (
	"os"
	"path/filepath"
	"testing"

	tea "github.com/charmbracelet/bubbletea"
)

func pressKey(t *testing.T, m boardModel, r rune) (boardModel, tea.Cmd) {
	t.Helper()
	updated, cmd := m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{r}})
	return updated.(boardModel), cmd
}

// 'E' with no milestone filter opens the current project in the editor.
func TestBoardEditProjectKey(t *testing.T) {
	s := newStore(t)
	_, _ = s.CreateProject("CLI", "Cliban", "old")

	m := newBoardModel(s, "CLI")
	updated, _ := m.Update(m.Init()())
	m = updated.(boardModel)

	m, cmd := pressKey(t, m, 'E')
	if cmd == nil {
		t.Fatal("expected editor cmd after 'E' press")
	}

	// Simulate the editor finishing with an edited buffer.
	path := filepath.Join(t.TempDir(), "project.md")
	_ = os.WriteFile(path, []byte("---\nname: Renamed\n---\nNew desc.\n"), 0o644)
	updated, refresh := m.Update(projectEditorFinishedMsg{tempPath: path})
	m = updated.(boardModel)
	if m.editorErr != nil {
		t.Fatalf("editorErr: %v", m.editorErr)
	}
	if refresh == nil {
		t.Error("expected refresh cmd after project editor finished")
	}
	p, err := s.GetProjectByKey("CLI")
	if err != nil {
		t.Fatal(err)
	}
	if p.Name != "Renamed" || p.Description != "New desc.\n" {
		t.Errorf("project not updated: name=%q desc=%q", p.Name, p.Description)
	}
}

// 'E' with an active milestone filter edits that milestone, not the project.
func TestBoardEditMilestoneKeyWhenFilterActive(t *testing.T) {
	s := newStore(t)
	_, _ = s.CreateProject("CLI", "Cliban", "")
	_, _ = s.CreateMilestone("CLI", "v0.1", "old", nil)

	m := newBoardModel(s, "CLI")
	updated, _ := m.Update(m.Init()())
	m = updated.(boardModel)
	m.milestoneFilter = "v0.1"

	m, cmd := pressKey(t, m, 'E')
	if cmd == nil {
		t.Fatal("expected editor cmd after 'E' press with filter active")
	}

	path := filepath.Join(t.TempDir(), "milestone.md")
	_ = os.WriteFile(path, []byte("---\nname: v0.1\nstatus: open\ntarget:\n---\nUpdated.\n"), 0o644)
	updated, _ = m.Update(milestoneEditorFinishedMsg{tempPath: path, editName: "v0.1"})
	m = updated.(boardModel)
	if m.editorErr != nil {
		t.Fatalf("editorErr: %v", m.editorErr)
	}
	ms, err := s.GetMilestone("CLI", "v0.1")
	if err != nil {
		t.Fatal(err)
	}
	if ms.Description != "Updated.\n" {
		t.Errorf("Description=%q want %q", ms.Description, "Updated.\n")
	}
}

// Renaming a milestone via the editor while it is the active filter keeps the
// filter pointing at the renamed milestone instead of an empty board.
func TestBoardMilestoneFilterFollowsRename(t *testing.T) {
	s := newStore(t)
	_, _ = s.CreateProject("CLI", "Cliban", "")
	_, _ = s.CreateMilestone("CLI", "v0.1", "", nil)

	m := newBoardModel(s, "CLI")
	updated, _ := m.Update(m.Init()())
	m = updated.(boardModel)
	m.milestoneFilter = "v0.1"

	path := filepath.Join(t.TempDir(), "milestone.md")
	_ = os.WriteFile(path, []byte("---\nname: v1.0\nstatus: open\ntarget:\n---\n"), 0o644)
	updated, _ = m.Update(milestoneEditorFinishedMsg{tempPath: path, editName: "v0.1"})
	m = updated.(boardModel)
	if m.editorErr != nil {
		t.Fatalf("editorErr: %v", m.editorErr)
	}
	if m.milestoneFilter != "v1.0" {
		t.Errorf("milestoneFilter=%q want v1.0 (followed rename)", m.milestoneFilter)
	}
}
