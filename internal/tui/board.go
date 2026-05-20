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
	store          *store.Store
	projectKey     string
	columns        [5][]*domain.Issue
	colCursor      int
	rowCursor      [5]int
	width          int
	back           bool
	err            error
	awaitingMove   bool
	filtering      bool
	filter         string
	openDetailKey  *domain.IssueKey
	showMilestones bool
	editorErr      error
	// milestoneFilter narrows the board to a single milestone when non-empty.
	// Cycled by pressing 'M' (uppercase).
	milestoneFilter string
	// milestonesOrdered lists every milestone in the current project in name order;
	// used to cycle the SELECTED card's milestone with 't', and to render milestone
	// names on cards. Refreshed by Init.
	milestonesOrdered []milestoneRef
	milestonesByID    map[int64]string
	// followKey is set before any mutation that may move the selected issue
	// (status change, within-column reorder). When the next boardLoadedMsg
	// arrives, the cursor is repositioned onto that issue's new row/column.
	followKey *domain.IssueKey
}

func newBoardModel(s *store.Store, key string) boardModel {
	return boardModel{store: s, projectKey: key}
}

type milestoneRef struct {
	ID   int64
	Name string
}

type boardLoadedMsg struct {
	cols       [5][]*domain.Issue
	milestones []milestoneRef
	err        error
}

func (m boardModel) Init() tea.Cmd {
	storeRef := m.store
	pk := m.projectKey
	milestone := m.milestoneFilter
	return func() tea.Msg {
		all, err := storeRef.ListIssues(store.ListIssuesFilter{ProjectKey: pk, MilestoneName: milestone})
		if err != nil {
			return boardLoadedMsg{err: err}
		}
		ms, _ := storeRef.ListMilestones(pk, "")
		refs := make([]milestoneRef, 0, len(ms))
		for _, m := range ms {
			refs = append(refs, milestoneRef{ID: m.ID, Name: m.Name})
		}
		msg := groupIssuesByStatus(all)
		msg.milestones = refs
		return msg
	}
}

func (m boardModel) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch v := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = v.Width
	case boardLoadedMsg:
		m.columns = v.cols
		m.err = v.err
		m.milestonesOrdered = v.milestones
		m.milestonesByID = make(map[int64]string, len(v.milestones))
		for _, r := range v.milestones {
			m.milestonesByID[r.ID] = r.Name
		}
		// Clamp cursors in case columns shrank.
		for i := 0; i < 5; i++ {
			if m.rowCursor[i] >= len(m.columns[i]) {
				m.rowCursor[i] = maxInt(0, len(m.columns[i])-1)
			}
		}
		// If a recent action requested cursor-follow, locate the issue and snap to it.
		if m.followKey != nil {
			for col := 0; col < 5; col++ {
				for row, issue := range m.columns[col] {
					if issue.Seq == m.followKey.Seq {
						m.colCursor = col
						m.rowCursor[col] = row
						break
					}
				}
			}
			m.followKey = nil
		}
	case tea.KeyMsg:
		return m.handleKey(v)
	case editorFinishedMsg:
		if v.err != nil {
			m.editorErr = v.err
			return m, m.Init() // still refresh in case of partial state
		}
		if applyErr := applyBufferToStore(m.store, m.projectKey, v.editKey, v.tempPath); applyErr != nil {
			m.editorErr = applyErr
		} else {
			m.editorErr = nil
		}
		return m, m.Init()
	case milestoneEditorFinishedMsg:
		if v.err != nil {
			m.editorErr = v.err
			return m, m.Init()
		}
		if applyErr := applyMilestoneBuffer(m.store, m.projectKey, v.tempPath); applyErr != nil {
			m.editorErr = applyErr
		} else {
			m.editorErr = nil
		}
		return m, m.Init()
	}
	return m, nil
}

func (m boardModel) handleKey(k tea.KeyMsg) (tea.Model, tea.Cmd) {
	if m.filtering {
		switch k.String() {
		case "esc":
			m.filtering = false
			m.filter = ""
		case "enter":
			m.filtering = false
		case "backspace":
			if len(m.filter) > 0 {
				m.filter = m.filter[:len(m.filter)-1]
			}
		default:
			if len(k.String()) == 1 {
				m.filter += k.String()
			}
		}
		return m, nil
	}
	if m.awaitingMove {
		m.awaitingMove = false
		st, ok := statusForKey(k.String())
		if !ok {
			return m, nil
		}
		m.followKey = m.selectedKey()
		return m, m.moveSelectedTo(st)
	}
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
	case "H":
		if cur := m.selected(); cur != nil && m.colCursor > 0 {
			m.followKey = m.selectedKey()
			return m, m.moveSelectedTo(domain.AllStatuses()[m.colCursor-1])
		}
	case "L":
		if cur := m.selected(); cur != nil && m.colCursor < 4 {
			m.followKey = m.selectedKey()
			return m, m.moveSelectedTo(domain.AllStatuses()[m.colCursor+1])
		}
	case "J":
		m.followKey = m.selectedKey()
		return m, m.shuffleSelected(+1)
	case "K":
		m.followKey = m.selectedKey()
		return m, m.shuffleSelected(-1)
	case " ":
		m.awaitingMove = true
	case "r":
		return m, m.Init()
	case "/":
		m.filtering = true
	case "enter":
		sel := m.selected()
		if sel != nil {
			key := domain.IssueKey{Project: m.projectKey, Seq: sel.Seq}
			m.openDetailKey = &key
		}
	case "m":
		m.showMilestones = !m.showMilestones
	case "M":
		milestones, _ := m.store.ListMilestones(m.projectKey, "")
		if len(milestones) == 0 {
			return m, nil
		}
		next := ""
		found := false
		for i, ms := range milestones {
			if ms.Name == m.milestoneFilter {
				if i+1 < len(milestones) {
					next = milestones[i+1].Name
				}
				found = true
				break
			}
		}
		if !found {
			next = milestones[0].Name
		}
		m.milestoneFilter = next
		return m, m.Init()
	case "e":
		sel := m.selected()
		if sel != nil {
			k := domain.IssueKey{Project: m.projectKey, Seq: sel.Seq}
			return m, openEditorForIssue(m.store, k)
		}
	case "n":
		return m, openEditorForNew(m.store, m.projectKey, domain.AllStatuses()[m.colCursor])
	case "N":
		return m, openEditorForNewMilestone(m.projectKey)
	case "t":
		sel := m.selected()
		if sel == nil {
			return m, nil
		}
		// Build the cycle: [none, ms1, ms2, ...]. Find current index, advance.
		curIdx := 0 // 0 = no milestone
		if sel.MilestoneID != nil {
			for i, r := range m.milestonesOrdered {
				if r.ID == *sel.MilestoneID {
					curIdx = i + 1
					break
				}
			}
		}
		nextIdx := (curIdx + 1) % (len(m.milestonesOrdered) + 1)
		key := domain.IssueKey{Project: m.projectKey, Seq: sel.Seq}
		m.followKey = &key
		var params store.UpdateIssueParams
		if nextIdx == 0 {
			params.ClearMilestone = true
		} else {
			name := m.milestonesOrdered[nextIdx-1].Name
			params.Milestone = &name
		}
		storeRef := m.store
		pk := m.projectKey
		ms := m.milestoneFilter
		return m, func() tea.Msg {
			if err := storeRef.UpdateIssue(key, params); err != nil {
				return boardLoadedMsg{err: err}
			}
			all, err := storeRef.ListIssues(store.ListIssuesFilter{ProjectKey: pk, MilestoneName: ms})
			if err != nil {
				return boardLoadedMsg{err: err}
			}
			out := groupIssuesByStatus(all)
			// Carry milestones through so they don't get wiped on this reload.
			mls, _ := storeRef.ListMilestones(pk, "")
			refs := make([]milestoneRef, 0, len(mls))
			for _, m := range mls {
				refs = append(refs, milestoneRef{ID: m.ID, Name: m.Name})
			}
			out.milestones = refs
			return out
		}
	case "a":
		sel := m.selected()
		if sel == nil {
			return m, nil
		}
		key := domain.IssueKey{Project: m.projectKey, Seq: sel.Seq}
		storeRef := m.store
		pk := m.projectKey
		return m, func() tea.Msg {
			if err := storeRef.SetIssueArchived(key, true); err != nil {
				return boardLoadedMsg{err: err}
			}
			all, err := storeRef.ListIssues(store.ListIssuesFilter{ProjectKey: pk})
			if err != nil {
				return boardLoadedMsg{err: err}
			}
			return groupIssuesByStatus(all)
		}
	}
	return m, nil
}

func (m boardModel) selected() *domain.Issue {
	col := m.columns[m.colCursor]
	if len(col) == 0 {
		return nil
	}
	if m.rowCursor[m.colCursor] >= len(col) {
		return nil
	}
	return col[m.rowCursor[m.colCursor]]
}

func (m boardModel) selectedKey() *domain.IssueKey {
	sel := m.selected()
	if sel == nil {
		return nil
	}
	return &domain.IssueKey{Project: m.projectKey, Seq: sel.Seq}
}

func (m boardModel) moveSelectedTo(st domain.Status) tea.Cmd {
	sel := m.selected()
	if sel == nil {
		return nil
	}
	key := domain.IssueKey{Project: m.projectKey, Seq: sel.Seq}
	storeRef := m.store
	pk := m.projectKey
	return func() tea.Msg {
		if err := storeRef.MoveIssue(key, st); err != nil {
			return boardLoadedMsg{err: err}
		}
		all, err := storeRef.ListIssues(store.ListIssuesFilter{ProjectKey: pk})
		if err != nil {
			return boardLoadedMsg{err: err}
		}
		return groupIssuesByStatus(all)
	}
}

func (m boardModel) shuffleSelected(delta int) tea.Cmd {
	col := m.columns[m.colCursor]
	idx := m.rowCursor[m.colCursor]
	target := idx + delta
	if target < 0 || target >= len(col) {
		return nil
	}
	sel := col[idx]
	other := col[target]
	storeRef := m.store
	pk := m.projectKey
	a := domain.IssueKey{Project: pk, Seq: sel.Seq}
	b := domain.IssueKey{Project: pk, Seq: other.Seq}
	posA, posB := sel.Position, other.Position
	return func() tea.Msg {
		if err := storeRef.SetIssuePosition(a, posB); err != nil {
			return boardLoadedMsg{err: err}
		}
		if err := storeRef.SetIssuePosition(b, posA); err != nil {
			return boardLoadedMsg{err: err}
		}
		all, err := storeRef.ListIssues(store.ListIssuesFilter{ProjectKey: pk})
		if err != nil {
			return boardLoadedMsg{err: err}
		}
		return groupIssuesByStatus(all)
	}
}

func groupIssuesByStatus(all []*domain.Issue) boardLoadedMsg {
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

func statusForKey(s string) (domain.Status, bool) {
	switch s {
	case "b":
		return domain.StatusBacklog, true
	case "i":
		return domain.StatusInProgress, true
	case "k":
		return domain.StatusBlocked, true
	case "r":
		return domain.StatusInReview, true
	case "d":
		return domain.StatusDone, true
	}
	return "", false
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

	filtered := m.columns
	if m.filter != "" {
		var fcols [5][]*domain.Issue
		needle := strings.ToLower(m.filter)
		for col := 0; col < 5; col++ {
			for _, issue := range m.columns[col] {
				if strings.Contains(strings.ToLower(issue.Title), needle) {
					fcols[col] = append(fcols[col], issue)
				}
			}
		}
		filtered = fcols
	}

	cols := make([]string, 5)
	for i := 0; i < 5; i++ {
		// Clamp cursor if filter shrinks column.
		rowCursor := m.rowCursor[i]
		if len(filtered[i]) > 0 && rowCursor >= len(filtered[i]) {
			rowCursor = len(filtered[i]) - 1
		}
		var b strings.Builder
		fmt.Fprintf(&b, "%s (%d)\n", headers[i], len(filtered[i]))
		for r, issue := range filtered[i] {
			titleLine := truncate(issue.Title, colWidth-2)
			milestoneLine := ""
			if issue.MilestoneID != nil {
				if name, ok := m.milestonesByID[*issue.MilestoneID]; ok {
					milestoneLine = "\n  " + StyleMuted.Render("⟜ "+truncate(name, colWidth-4))
				}
			}
			card := fmt.Sprintf("%s-%d %s\n  %s%s",
				m.projectKey, issue.Seq, PriorityDot(string(issue.Priority)),
				titleLine, milestoneLine)
			if i == m.colCursor && r == rowCursor {
				card = StyleSelected.Render(card)
			}
			b.WriteString(card + "\n")
		}
		cols[i] = StyleColumn.Width(colWidth).Render(b.String())
	}
	body := lipgloss.JoinHorizontal(lipgloss.Top, cols...)

	helpText := "hjkl move  enter detail  e edit  n new  N ms+  t tag-ms  Space mv  a archive  M ms-filter  / filter  r refresh  q quit"
	if m.milestoneFilter != "" {
		helpText = fmt.Sprintf("milestone: %s  | %s", m.milestoneFilter, helpText)
	}
	if m.filter != "" || m.filtering {
		helpText = fmt.Sprintf("filter: %s  | %s", m.filter, helpText)
	}
	if m.editorErr != nil {
		helpText = "ERR: " + m.editorErr.Error() + " | " + helpText
	}
	help := StyleStatusBar.Render(helpText)
	base := StyleTitle.Render(fmt.Sprintf("cliban — %s", m.projectKey)) + "\n" + body + "\n" + help

	if m.showMilestones {
		return base + "\n" + renderMilestoneOverlay(m.store, m.projectKey)
	}
	return base
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
