package tui

import (
	"fmt"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/alex/cliban/internal/domain"
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

// TestBoardTagCyclesIssueMilestone verifies 't' cycles the selected card's
// milestone through none → v0.1 → v0.2 → none.
func TestBoardTagCyclesIssueMilestone(t *testing.T) {
	s := newStore(t)
	_, _ = s.CreateProject("CLI", "Cliban", "")
	_, _ = s.CreateMilestone("CLI", "v0.1", "", nil)
	_, _ = s.CreateMilestone("CLI", "v0.2", "", nil)
	i, _ := s.CreateIssue(store.CreateIssueParams{ProjectKey: "CLI", Title: "alpha"})

	m := newBoardModel(s, "CLI")
	updated, _ := m.Update(m.Init()())
	m = updated.(boardModel)

	currentMilestone := func() string {
		got, err := s.GetIssueByKey(domain.IssueKey{Project: "CLI", Seq: i.Seq})
		if err != nil {
			t.Fatal(err)
		}
		if got.MilestoneID == nil {
			return ""
		}
		return m.milestonesByID[*got.MilestoneID]
	}

	press := func() {
		updated, cmd := m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'t'}})
		m = updated.(boardModel)
		if cmd == nil {
			t.Fatal("expected cmd after 't' press")
		}
		updated, _ = m.Update(cmd())
		m = updated.(boardModel)
	}

	if currentMilestone() != "" {
		t.Fatalf("initial milestone=%q want empty", currentMilestone())
	}
	press()
	if got := currentMilestone(); got != "v0.1" {
		t.Errorf("after press 1: milestone=%q want v0.1", got)
	}
	press()
	if got := currentMilestone(); got != "v0.2" {
		t.Errorf("after press 2: milestone=%q want v0.2", got)
	}
	press()
	if got := currentMilestone(); got != "" {
		t.Errorf("after press 3: milestone=%q want '' (cleared)", got)
	}
}

// TestBoardVerticalScrollAdjustsToCursor verifies that when a column has more
// cards than fit in the viewport, moving the cursor downward scrolls the
// column so the cursor stays visible.
func TestBoardVerticalScrollAdjustsToCursor(t *testing.T) {
	s := newStore(t)
	_, _ = s.CreateProject("CLI", "Cliban", "")
	for i := 0; i < 30; i++ {
		_, _ = s.CreateIssue(store.CreateIssueParams{ProjectKey: "CLI", Title: fmt.Sprintf("t-%d", i)})
	}

	m := newBoardModel(s, "CLI")
	m.width, m.height = 160, 16 // tight height → rowBudget ≈ 9, cardBudget ≈ 7
	updated, _ := m.Update(m.Init()())
	m = updated.(boardModel)
	if m.rowScroll[0] != 0 {
		t.Fatalf("initial rowScroll=%d, want 0", m.rowScroll[0])
	}

	for i := 0; i < 25; i++ {
		updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'j'}})
		m = updated.(boardModel)
	}
	if m.rowCursor[0] != 25 {
		t.Fatalf("rowCursor=%d, want 25", m.rowCursor[0])
	}
	if m.rowScroll[0] == 0 {
		t.Errorf("rowScroll stayed 0 after moving cursor to row 25; column should have scrolled")
	}
	if m.rowScroll[0] > m.rowCursor[0] {
		t.Errorf("rowScroll=%d past rowCursor=%d", m.rowScroll[0], m.rowCursor[0])
	}

	for i := 0; i < 25; i++ {
		updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'k'}})
		m = updated.(boardModel)
	}
	if m.rowScroll[0] != 0 {
		t.Errorf("after scrolling back up rowScroll=%d, want 0", m.rowScroll[0])
	}
}

// TestBoardHorizontalScrollAdjustsToCursor verifies that when fewer than five
// columns fit in the terminal width, moving the column cursor right scrolls
// the column strip so the selected column stays visible.
func TestBoardHorizontalScrollAdjustsToCursor(t *testing.T) {
	s := newStore(t)
	_, _ = s.CreateProject("CLI", "Cliban", "")
	_, _ = s.CreateIssue(store.CreateIssueParams{ProjectKey: "CLI", Title: "x"})

	m := newBoardModel(s, "CLI")
	m.width, m.height = 70, 30 // slot = 26+4 = 30, so only 2 columns fit
	updated, _ := m.Update(m.Init()())
	m = updated.(boardModel)

	_, colsToShow, _ := m.viewport()
	if colsToShow >= 5 {
		t.Fatalf("expected fewer than 5 columns visible at width 70, got %d", colsToShow)
	}
	if m.colScrollH != 0 {
		t.Fatalf("initial colScrollH=%d, want 0", m.colScrollH)
	}

	for i := 0; i < 4; i++ {
		updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'l'}})
		m = updated.(boardModel)
	}
	if m.colCursor != 4 {
		t.Fatalf("colCursor=%d, want 4", m.colCursor)
	}
	if m.colScrollH == 0 {
		t.Errorf("colScrollH stayed 0 after cursor reached col 4; strip should have scrolled")
	}
	if m.colCursor < m.colScrollH || m.colCursor >= m.colScrollH+colsToShow {
		t.Errorf("colCursor=%d not in visible window [%d,%d)", m.colCursor, m.colScrollH, m.colScrollH+colsToShow)
	}

	for i := 0; i < 4; i++ {
		updated, _ = m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'h'}})
		m = updated.(boardModel)
	}
	if m.colScrollH != 0 {
		t.Errorf("after returning cursor to col 0, colScrollH=%d, want 0", m.colScrollH)
	}
}

func TestRenderMarquee(t *testing.T) {
	t.Run("short title returned unchanged", func(t *testing.T) {
		got := renderMarquee("hello", 10, 5)
		if got != "hello" {
			t.Errorf("got %q, want %q", got, "hello")
		}
	})
	t.Run("equal-length title returned unchanged", func(t *testing.T) {
		got := renderMarquee("0123456789", 10, 0)
		if got != "0123456789" {
			t.Errorf("got %q, want %q", got, "0123456789")
		}
	})
	t.Run("overflow at offset 0 shows window start", func(t *testing.T) {
		got := renderMarquee("Long title that doesn't fit", 10, 0)
		want := "Long title"
		if got != want {
			t.Errorf("got %q, want %q", got, want)
		}
	})
	t.Run("overflow advances by one rune per offset", func(t *testing.T) {
		got := renderMarquee("Long title that doesn't fit", 10, 1)
		want := "ong title "
		if got != want {
			t.Errorf("got %q, want %q", got, want)
		}
	})
	t.Run("cycle wraps with separator and repeats title", func(t *testing.T) {
		// "abc" + "   •   " (7 runes) = 10-rune cycle. At offset 3 the window
		// of width 10 covers separator + start of next title.
		got := renderMarquee("abc", 3, 0) // short — returned as-is
		if got != "abc" {
			t.Errorf("short-title got %q, want %q", got, "abc")
		}
		// Force scrolling by passing a width smaller than the title.
		got = renderMarquee("abcdef", 4, 6) // cycle is "abcdef   •   " (13 runes); offset 6 lands on separator start
		want := "   •"
		if got != want {
			t.Errorf("got %q, want %q", got, want)
		}
	})
	t.Run("width zero returns empty", func(t *testing.T) {
		if got := renderMarquee("anything", 0, 0); got != "" {
			t.Errorf("got %q, want empty", got)
		}
	})
}

// TestBoardSnapCursorToKey verifies that snapCursorToKey moves colCursor and
// rowCursor onto the card whose key matches the argument, regardless of which
// column it currently lives in. Constructs a boardModel literal with
// pre-populated columns to avoid DB bootstrap.
func TestBoardSnapCursorToKey(t *testing.T) {
	m := boardModel{
		projectKey: "XX",
		columns: [5][]*domain.Issue{
			0: {{Seq: 1, Title: "a"}},
			1: {{Seq: 2, Title: "b"}},
			2: {{Seq: 3, Title: "c"}},
		},
	}
	m.snapCursorToKey("XX-2")
	if m.colCursor != 1 {
		t.Fatalf("expected col=1, got %d", m.colCursor)
	}
	if m.rowCursor[1] != 0 {
		t.Fatalf("expected row=0, got %d", m.rowCursor[1])
	}

	// Snap to a key in a different column resets the cursor accordingly.
	m.snapCursorToKey("XX-3")
	if m.colCursor != 2 {
		t.Fatalf("expected col=2 after snapping to XX-3, got %d", m.colCursor)
	}
	if m.rowCursor[2] != 0 {
		t.Fatalf("expected row=0 in col 2, got %d", m.rowCursor[2])
	}

	// Snapping to a missing key is a silent no-op.
	prevCol, prevRow := m.colCursor, m.rowCursor[m.colCursor]
	m.snapCursorToKey("XX-99")
	if m.colCursor != prevCol || m.rowCursor[m.colCursor] != prevRow {
		t.Fatalf("missing key changed cursor: col %d->%d row %d->%d",
			prevCol, m.colCursor, prevRow, m.rowCursor[m.colCursor])
	}

	// Snapping to a key from a different project is also a no-op.
	m.snapCursorToKey("YY-1")
	if m.colCursor != prevCol {
		t.Fatalf("cross-project key changed cursor: col %d->%d", prevCol, m.colCursor)
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
