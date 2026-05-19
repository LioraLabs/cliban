package tui

import (
	"fmt"

	"github.com/alex/cliban/internal/store"
	tea "github.com/charmbracelet/bubbletea"
)

type view int

const (
	viewProjects view = iota
	viewBoard
	viewIssue
)

type Model struct {
	store    *store.Store
	view     view
	projects projectsModel
	board    boardModel
	issue    issueModel
	width    int
	height   int
}

func NewModel(s *store.Store) Model {
	return Model{store: s, view: viewProjects, projects: newProjectsModel(s)}
}

func (m Model) Init() tea.Cmd { return m.projects.Init() }

func (m Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch v := msg.(type) {
	case tea.WindowSizeMsg:
		m.width, m.height = v.Width, v.Height
	case tea.KeyMsg:
		if v.String() == "ctrl+c" {
			return m, tea.Quit
		}
	}
	var cmd tea.Cmd
	switch m.view {
	case viewProjects:
		var nm tea.Model
		nm, cmd = m.projects.Update(msg)
		m.projects = nm.(projectsModel)
		if m.projects.selected != "" {
			m.board = newBoardModel(m.store, m.projects.selected)
			m.view = viewBoard
			m.projects.selected = ""
			return m, m.board.Init()
		}
	case viewBoard:
		var nm tea.Model
		nm, cmd = m.board.Update(msg)
		m.board = nm.(boardModel)
		if m.board.back {
			m.view = viewProjects
			m.board.back = false
		}
		if m.board.openDetailKey != nil {
			m.issue = newIssueModel(m.store, *m.board.openDetailKey)
			m.board.openDetailKey = nil
			m.view = viewIssue
			return m, m.issue.Init()
		}
	case viewIssue:
		var nm tea.Model
		nm, cmd = m.issue.Update(msg)
		m.issue = nm.(issueModel)
		if m.issue.back {
			m.view = viewBoard
			m.issue.back = false
		}
	}
	return m, cmd
}

func (m Model) View() string {
	switch m.view {
	case viewProjects:
		return m.projects.View()
	case viewBoard:
		return m.board.View()
	case viewIssue:
		return m.issue.View()
	default:
		return fmt.Sprintf("unknown view")
	}
}


