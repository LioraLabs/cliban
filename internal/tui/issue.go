package tui

import (
	"fmt"
	"strings"

	"github.com/alex/cliban/internal/domain"
	"github.com/alex/cliban/internal/store"
	tea "github.com/charmbracelet/bubbletea"
)

type issueModel struct {
	store *store.Store
	key   domain.IssueKey
	issue *domain.Issue
	subs  []*domain.Issue
	back  bool
	err   error
}

func newIssueModel(s *store.Store, k domain.IssueKey) issueModel {
	return issueModel{store: s, key: k}
}

type issueLoadedMsg struct {
	issue *domain.Issue
	subs  []*domain.Issue
	err   error
}

func (m issueModel) Init() tea.Cmd {
	storeRef := m.store
	k := m.key
	return func() tea.Msg {
		i, err := storeRef.GetIssueByKey(k)
		if err != nil {
			return issueLoadedMsg{err: err}
		}
		subs, err := storeRef.ListIssues(store.ListIssuesFilter{ProjectKey: k.Project, ParentKey: &k})
		if err != nil {
			return issueLoadedMsg{err: err}
		}
		return issueLoadedMsg{issue: i, subs: subs}
	}
}

func (m issueModel) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch v := msg.(type) {
	case issueLoadedMsg:
		m.issue, m.subs, m.err = v.issue, v.subs, v.err
	case editorFinishedMsg:
		if v.err != nil {
			m.err = v.err
			return m, m.Init()
		}
		m.err = applyBufferToStore(m.store, m.key.Project, v.editKey, v.tempPath)
		return m, m.Init()
	case tea.KeyMsg:
		switch v.String() {
		case "q", "esc":
			m.back = true
		case "e":
			return m, openEditorForIssue(m.store, m.key)
		}
	}
	return m, nil
}

func (m issueModel) View() string {
	if m.err != nil {
		return "error: " + m.err.Error()
	}
	if m.issue == nil {
		return "loading…"
	}
	var sb strings.Builder
	fmt.Fprintf(&sb, "%s — %s\n", m.key, m.issue.Title)
	fmt.Fprintf(&sb, "%s\n", StyleMuted.Render(fmt.Sprintf("status: %s   priority: %s", m.issue.Status, m.issue.Priority)))
	sb.WriteString("\n")
	sb.WriteString(m.issue.Description)
	sb.WriteString("\n\n")
	if len(m.subs) > 0 {
		sb.WriteString(StyleTitle.Render("sub-issues") + "\n")
		for _, sub := range m.subs {
			fmt.Fprintf(&sb, "  %s-%d  %s  %s\n", m.key.Project, sub.Seq, sub.Status, sub.Title)
		}
	}
	sb.WriteString("\n" + StyleStatusBar.Render("q/esc back  e edit"))
	return sb.String()
}
