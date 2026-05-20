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

// TestBoardCursorFollowsMove verifies that after moving an issue to a new
// column, the cursor lands on that issue's new location instead of staying
// in the old column on whatever happens to be there.
func TestBoardCursorFollowsMove(t *testing.T) {
	s := newStore(t)
	_, _ = s.CreateProject("CLI", "Cliban", "")
	a, _ := s.CreateIssue(store.CreateIssueParams{ProjectKey: "CLI", Title: "alpha"})
	_, _ = s.CreateIssue(store.CreateIssueParams{ProjectKey: "CLI", Title: "beta"})

	m := newBoardModel(s, "CLI")
	loadCmd := m.Init()
	updated, _ := m.Update(loadCmd())
	m = updated.(boardModel)

	if sel := m.selected(); sel == nil || sel.Seq != a.Seq {
		t.Fatalf("initial selection: want alpha (seq %d), got %+v", a.Seq, sel)
	}

	// Press Space then 'd' to move alpha to DONE.
	updated, _ = m.Update(tea.KeyMsg{Type: tea.KeySpace})
	m = updated.(boardModel)
	updated, moveCmd := m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'d'}})
	m = updated.(boardModel)
	if moveCmd == nil {
		t.Fatal("expected a move command")
	}
	// Run the move command (talks to the store) and feed the result back in.
	updated, _ = m.Update(moveCmd())
	m = updated.(boardModel)

	if m.colCursor != 4 {
		t.Errorf("colCursor = %d, want 4 (DONE)", m.colCursor)
	}
	sel := m.selected()
	if sel == nil || sel.Seq != a.Seq {
		t.Errorf("selected after move = %+v, want alpha (seq %d)", sel, a.Seq)
	}
}

// TestBoardMilestoneFilterCycles verifies that pressing 'M' cycles through
// All → v0.1 → v0.2 → All and the board content is filtered accordingly.
func TestBoardMilestoneFilterCycles(t *testing.T) {
	s := newStore(t)
	_, _ = s.CreateProject("CLI", "Cliban", "")
	_, _ = s.CreateMilestone("CLI", "v0.1", "", nil)
	_, _ = s.CreateMilestone("CLI", "v0.2", "", nil)
	_, _ = s.CreateIssue(store.CreateIssueParams{ProjectKey: "CLI", Title: "in v0.1", MilestoneName: "v0.1"})
	_, _ = s.CreateIssue(store.CreateIssueParams{ProjectKey: "CLI", Title: "in v0.2", MilestoneName: "v0.2"})
	_, _ = s.CreateIssue(store.CreateIssueParams{ProjectKey: "CLI", Title: "no milestone"})

	m := newBoardModel(s, "CLI")
	updated, _ := m.Update(m.Init()())
	m = updated.(boardModel)

	totalVisible := func() int {
		n := 0
		for _, col := range m.columns {
			n += len(col)
		}
		return n
	}
	if got := totalVisible(); got != 3 {
		t.Fatalf("initial visible=%d, want 3 (no filter)", got)
	}

	press := func() {
		updated, cmd := m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'M'}})
		m = updated.(boardModel)
		if cmd == nil {
			t.Fatal("expected cmd after M press")
		}
		updated, _ = m.Update(cmd())
		m = updated.(boardModel)
	}

	press()
	if m.milestoneFilter != "v0.1" {
		t.Errorf("after press 1: filter=%q want v0.1", m.milestoneFilter)
	}
	if got := totalVisible(); got != 1 {
		t.Errorf("v0.1 visible=%d want 1", got)
	}

	press()
	if m.milestoneFilter != "v0.2" {
		t.Errorf("after press 2: filter=%q want v0.2", m.milestoneFilter)
	}
	if got := totalVisible(); got != 1 {
		t.Errorf("v0.2 visible=%d want 1", got)
	}

	press()
	if m.milestoneFilter != "" {
		t.Errorf("after press 3: filter=%q want '' (cycled to All)", m.milestoneFilter)
	}
	if got := totalVisible(); got != 3 {
		t.Errorf("all visible=%d want 3", got)
	}
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
