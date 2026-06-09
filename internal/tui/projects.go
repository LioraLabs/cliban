package tui

import (
	"fmt"
	"strings"

	"github.com/alex/cliban/internal/domain"
	"github.com/alex/cliban/internal/store"
	tea "github.com/charmbracelet/bubbletea"
)

type projectsModel struct {
	store    *store.Store
	items    []*domain.Project
	cursor   int
	selected string
	err      error
}

func newProjectsModel(s *store.Store) projectsModel {
	return projectsModel{store: s}
}

type projectsLoadedMsg struct {
	items []*domain.Project
	err   error
}

func (m projectsModel) Init() tea.Cmd {
	storeRef := m.store
	return func() tea.Msg {
		ps, err := storeRef.ListProjects(false)
		return projectsLoadedMsg{items: ps, err: err}
	}
}

func (m projectsModel) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch v := msg.(type) {
	case projectsLoadedMsg:
		m.items = v.items
		m.err = v.err
	case projectEditorFinishedMsg:
		if v.err != nil {
			m.err = v.err
			return m, m.Init()
		}
		m.err = applyProjectBuffer(m.store, v.projectKey, v.tempPath)
		return m, m.Init()
	case tea.KeyMsg:
		switch v.String() {
		case "j", "down":
			if m.cursor < len(m.items)-1 {
				m.cursor++
			}
		case "k", "up":
			if m.cursor > 0 {
				m.cursor--
			}
		case "enter":
			if len(m.items) > 0 {
				m.selected = m.items[m.cursor].Key
			}
		case "E":
			if len(m.items) > 0 {
				return m, openEditorForProject(m.store, m.items[m.cursor].Key)
			}
		case "q":
			return m, tea.Quit
		}
	}
	return m, nil
}

func (m projectsModel) View() string {
	var sb strings.Builder
	sb.WriteString(StyleTitle.Render("cliban — projects") + "\n\n")
	if m.err != nil {
		sb.WriteString("error: " + m.err.Error() + "\n")
		return sb.String()
	}
	if len(m.items) == 0 {
		sb.WriteString(StyleMuted.Render("no projects. `cliban project add <KEY> --name ...`") + "\n")
	}
	for i, p := range m.items {
		line := fmt.Sprintf("  %-8s %s", p.Key, p.Name)
		if i == m.cursor {
			line = StyleSelected.Render("▸ " + strings.TrimLeft(line, " "))
		}
		sb.WriteString(line + "\n")
	}
	sb.WriteString("\n" + StyleStatusBar.Render("j/k move  enter open  E edit  q quit"))
	return sb.String()
}
