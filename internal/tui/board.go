package tui

import (
	"fmt"
	"strings"

	"github.com/alex/cliban/internal/domain"
	"github.com/alex/cliban/internal/store"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
)

type boardModel struct {
	store      *store.Store
	projectKey string
	columns    [5][]*domain.Issue
	colCursor  int
	rowCursor  [5]int
	width      int
	back       bool
	err        error
}

func newBoardModel(s *store.Store, key string) boardModel {
	return boardModel{store: s, projectKey: key}
}

type boardLoadedMsg struct {
	cols [5][]*domain.Issue
	err  error
}

func (m boardModel) Init() tea.Cmd {
	storeRef := m.store
	pk := m.projectKey
	return func() tea.Msg {
		all, err := storeRef.ListIssues(store.ListIssuesFilter{ProjectKey: pk})
		if err != nil {
			return boardLoadedMsg{err: err}
		}
		var cols [5][]*domain.Issue
		statuses := domain.AllStatuses()
		for _, i := range all {
			for idx, s := range statuses {
				if i.Status == s {
					cols[idx] = append(cols[idx], i)
				}
			}
		}
		return boardLoadedMsg{cols: cols}
	}
}

func (m boardModel) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch v := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = v.Width
	case boardLoadedMsg:
		m.columns = v.cols
		m.err = v.err
		// Clamp cursors in case columns shrank.
		for i := 0; i < 5; i++ {
			if m.rowCursor[i] >= len(m.columns[i]) {
				m.rowCursor[i] = maxInt(0, len(m.columns[i])-1)
			}
		}
	case tea.KeyMsg:
		return m.handleKey(v)
	}
	return m, nil
}

func (m boardModel) handleKey(k tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch k.String() {
	case "q", "esc":
		m.back = true
	case "h", "left":
		if m.colCursor > 0 {
			m.colCursor--
		}
	case "l", "right":
		if m.colCursor < 4 {
			m.colCursor++
		}
	case "j", "down":
		if m.rowCursor[m.colCursor] < len(m.columns[m.colCursor])-1 {
			m.rowCursor[m.colCursor]++
		}
	case "k", "up":
		if m.rowCursor[m.colCursor] > 0 {
			m.rowCursor[m.colCursor]--
		}
	case "r":
		return m, m.Init()
	}
	return m, nil
}

func (m boardModel) View() string {
	headers := []string{"BACKLOG", "IN-PROG", "BLOCKED", "IN-REV", "DONE"}
	colWidth := 22
	if m.width > 0 {
		colWidth = (m.width - 8) / 5
		if colWidth < 14 {
			colWidth = 14
		}
	}
	cols := make([]string, 5)
	for i := 0; i < 5; i++ {
		var b strings.Builder
		fmt.Fprintf(&b, "%s (%d)\n", headers[i], len(m.columns[i]))
		for r, issue := range m.columns[i] {
			card := fmt.Sprintf("%s-%d %s\n  %s",
				m.projectKey, issue.Seq, PriorityDot(string(issue.Priority)),
				truncate(issue.Title, colWidth-2))
			if i == m.colCursor && r == m.rowCursor[i] {
				card = StyleSelected.Render(card)
			}
			b.WriteString(card + "\n")
		}
		cols[i] = StyleColumn.Width(colWidth).Render(b.String())
	}
	body := lipgloss.JoinHorizontal(lipgloss.Top, cols...)
	help := StyleStatusBar.Render("hjkl move  enter detail  e edit  n new  Space mv  / filter  r refresh  q quit")
	return StyleTitle.Render(fmt.Sprintf("cliban — %s", m.projectKey)) + "\n" + body + "\n" + help
}

func truncate(s string, n int) string {
	if n < 4 {
		return s
	}
	if len(s) <= n {
		return s
	}
	return s[:n-1] + "…"
}

func maxInt(a, b int) int {
	if a > b {
		return a
	}
	return b
}
