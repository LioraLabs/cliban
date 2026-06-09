package tui

import (
	"strings"
	"testing"

	tea "github.com/charmbracelet/bubbletea"
)

func overlayBoard(t *testing.T) boardModel {
	t.Helper()
	s := newStore(t)
	_, _ = s.CreateProject("CLI", "Cliban", "")
	_, _ = s.CreateMilestone("CLI", "v0.1", "", nil)
	_, _ = s.CreateMilestone("CLI", "v0.2", "", nil)
	m := newBoardModel(s, "CLI")
	updated, _ := m.Update(m.Init()())
	return updated.(boardModel)
}

// While the milestone overlay is open, j/k move the overlay cursor instead of
// the board cursor, and esc closes the overlay.
func TestMilestoneOverlayCursorIsModal(t *testing.T) {
	m := overlayBoard(t)

	m, _ = pressKey(t, m, 'm')
	if !m.showMilestones {
		t.Fatal("overlay should be open after 'm'")
	}
	if m.msCursor != 0 {
		t.Fatalf("initial msCursor=%d want 0", m.msCursor)
	}

	m, _ = pressKey(t, m, 'j')
	if m.msCursor != 1 {
		t.Errorf("msCursor=%d want 1 after j", m.msCursor)
	}
	if m.rowCursor[0] != 0 {
		t.Errorf("board rowCursor moved while overlay open")
	}
	// Clamps at the end of the list.
	m, _ = pressKey(t, m, 'j')
	if m.msCursor != 1 {
		t.Errorf("msCursor=%d want 1 (clamped)", m.msCursor)
	}
	m, _ = pressKey(t, m, 'k')
	if m.msCursor != 0 {
		t.Errorf("msCursor=%d want 0 after k", m.msCursor)
	}

	updated, _ := m.Update(tea.KeyMsg{Type: tea.KeyEsc})
	m = updated.(boardModel)
	if m.showMilestones {
		t.Error("overlay should close on esc")
	}
}

// 'E' while the overlay is open returns an editor cmd for the highlighted
// milestone.
func TestMilestoneOverlayEditKey(t *testing.T) {
	m := overlayBoard(t)
	m, _ = pressKey(t, m, 'm')
	m, _ = pressKey(t, m, 'j')
	m, cmd := pressKey(t, m, 'E')
	if cmd == nil {
		t.Fatal("expected editor cmd after 'E' on overlay")
	}
}

// The overlay render marks the milestone under the cursor.
func TestMilestoneOverlayRendersCursor(t *testing.T) {
	m := overlayBoard(t)
	m, _ = pressKey(t, m, 'm')
	m, _ = pressKey(t, m, 'j')
	view := m.View()
	if !strings.Contains(view, "▸ v0.2") {
		t.Errorf("overlay should highlight v0.2; view:\n%s", view)
	}
}
