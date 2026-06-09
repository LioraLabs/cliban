package tui

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/alex/cliban/internal/domain"
	"github.com/alex/cliban/internal/store"
	tea "github.com/charmbracelet/bubbletea"
)

// 'E' on the projects list opens the selected project in the editor, and the
// finished msg applies the buffer and refreshes the list.
func TestProjectsListEditKey(t *testing.T) {
	s := newStore(t)
	_, _ = s.CreateProject("CLI", "Cliban", "old")

	m := newProjectsModel(s)
	updated, _ := m.Update(m.Init()())
	m = updated.(projectsModel)

	updated, cmd := m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'E'}})
	m = updated.(projectsModel)
	if cmd == nil {
		t.Fatal("expected editor cmd after 'E' press")
	}

	path := filepath.Join(t.TempDir(), "project.md")
	_ = os.WriteFile(path, []byte("---\nname: Renamed\n---\nNew desc.\n"), 0o644)
	updated, refresh := m.Update(projectEditorFinishedMsg{tempPath: path, projectKey: "CLI"})
	m = updated.(projectsModel)
	if m.err != nil {
		t.Fatalf("err: %v", m.err)
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

// 'e' on the issue detail view opens the issue in the editor (the footer has
// always advertised this), and the finished msg applies and refreshes.
func TestIssueDetailEditKey(t *testing.T) {
	s := newStore(t)
	_, _ = s.CreateProject("CLI", "Cliban", "")
	i, _ := s.CreateIssue(store.CreateIssueParams{ProjectKey: "CLI", Title: "alpha"})
	key := domain.IssueKey{Project: "CLI", Seq: i.Seq}

	m := newIssueModel(s, key)
	updated, _ := m.Update(m.Init()())
	m = updated.(issueModel)

	updated, cmd := m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'e'}})
	m = updated.(issueModel)
	if cmd == nil {
		t.Fatal("expected editor cmd after 'e' press")
	}

	path := filepath.Join(t.TempDir(), "issue.md")
	_ = os.WriteFile(path, []byte("---\ntitle: beta\nstatus: backlog\npriority: none\nmilestone:\nparent:\n---\nEdited.\n"), 0o644)
	updated, refresh := m.Update(editorFinishedMsg{tempPath: path, editKey: &key})
	m = updated.(issueModel)
	if m.err != nil {
		t.Fatalf("err: %v", m.err)
	}
	if refresh == nil {
		t.Error("expected refresh cmd after editor finished")
	}
	got, err := s.GetIssueByKey(key)
	if err != nil {
		t.Fatal(err)
	}
	if got.Title != "beta" || got.Description != "Edited.\n" {
		t.Errorf("issue not updated: title=%q desc=%q", got.Title, got.Description)
	}
}

// Returning from the issue detail view to the board refreshes the board so
// edits made in the detail view show up.
func TestAppRefreshesBoardOnBackFromIssue(t *testing.T) {
	s := newStore(t)
	_, _ = s.CreateProject("CLI", "Cliban", "")
	i, _ := s.CreateIssue(store.CreateIssueParams{ProjectKey: "CLI", Title: "alpha"})
	key := domain.IssueKey{Project: "CLI", Seq: i.Seq}

	app := NewModel(s)
	app.view = viewIssue
	app.board = newBoardModel(s, "CLI")
	app.issue = newIssueModel(s, key)

	updated, cmd := app.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'q'}})
	app = updated.(Model)
	if app.view != viewBoard {
		t.Fatalf("view=%v want viewBoard", app.view)
	}
	if cmd == nil {
		t.Error("expected board refresh cmd when returning from issue detail")
	}
}
