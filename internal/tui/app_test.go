package tui

import (
	"testing"

	tea "github.com/charmbracelet/bubbletea"
)

// TestRootForwardsSizeToBoardOnProjectSelect guards against the regression
// where opening a board from the project list left the new boardModel with
// width=0/height=0, defeating the scroll math (Bubble Tea only sends
// WindowSizeMsg on actual resize, so the new view never learned its size).
func TestRootForwardsSizeToBoardOnProjectSelect(t *testing.T) {
	s := newStore(t)
	_, _ = s.CreateProject("CLI", "Cliban", "")

	root := NewModel(s)
	// Initial layout — what Bubble Tea would send right after start.
	updated, _ := root.Update(tea.WindowSizeMsg{Width: 140, Height: 40})
	root = updated.(Model)
	// Load the project list.
	updated, _ = root.Update(root.projects.Init()())
	root = updated.(Model)
	// Open the project.
	updated, _ = root.Update(tea.KeyMsg{Type: tea.KeyEnter})
	root = updated.(Model)

	if root.view != viewBoard {
		t.Fatalf("view=%v, want viewBoard", root.view)
	}
	if root.board.width != 140 || root.board.height != 40 {
		t.Errorf("board size after project select = (%d,%d), want (140,40)",
			root.board.width, root.board.height)
	}
}
