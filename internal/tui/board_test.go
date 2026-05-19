package tui

import (
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/alex/cliban/internal/store"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/x/exp/teatest"
)

func newStore(t *testing.T) *store.Store {
	t.Helper()
	s, err := store.Open(filepath.Join(t.TempDir(), "t.db"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { s.Close() })
	if err := s.Migrate(); err != nil {
		t.Fatal(err)
	}
	return s
}

func TestBoardRendersColumns(t *testing.T) {
	s := newStore(t)
	_, _ = s.CreateProject("CLI", "Cliban", "")
	_, _ = s.CreateIssue(store.CreateIssueParams{ProjectKey: "CLI", Title: "first"})

	m := newBoardModel(s, "CLI")
	tm := teatest.NewTestModel(t, m, teatest.WithInitialTermSize(120, 30))
	teatest.WaitFor(t, tm.Output(), func(b []byte) bool {
		return strings.Contains(string(b), "BACKLOG")
	}, teatest.WithCheckInterval(50*time.Millisecond), teatest.WithDuration(2*time.Second))
	// boardModel only signals "back" via state; the parent root.Model handles quit.
	// In isolation we force-quit by sending tea.QuitMsg directly.
	tm.Send(tea.QuitMsg{})
	tm.WaitFinished(t, teatest.WithFinalTimeout(2*time.Second))
}

func TestBoardSpaceMovesIssueToDone(t *testing.T) {
	s := newStore(t)
	_, _ = s.CreateProject("CLI", "Cliban", "")
	_, _ = s.CreateIssue(store.CreateIssueParams{ProjectKey: "CLI", Title: "first"})

	m := newBoardModel(s, "CLI")
	tm := teatest.NewTestModel(t, m, teatest.WithInitialTermSize(120, 30))
	teatest.WaitFor(t, tm.Output(), func(b []byte) bool {
		return strings.Contains(string(b), "first")
	}, teatest.WithCheckInterval(50*time.Millisecond), teatest.WithDuration(2*time.Second))
	tm.Send(tea.KeyMsg{Type: tea.KeySpace})
	tm.Send(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'d'}})
	teatest.WaitFor(t, tm.Output(), func(b []byte) bool {
		return strings.Contains(string(b), "DONE (1)")
	}, teatest.WithCheckInterval(50*time.Millisecond), teatest.WithDuration(2*time.Second))
	tm.Send(tea.QuitMsg{})
	tm.WaitFinished(t, teatest.WithFinalTimeout(2*time.Second))
}
