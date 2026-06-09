package tui

import (
	"fmt"
	"strings"
	"time"

	"github.com/alex/cliban/internal/domain"
	"github.com/alex/cliban/internal/store"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
)

// boardColChrome is the column's non-content overhead: rounded border (2) +
// horizontal padding (2). Only one column is ever visible at a time; it
// expands to fill the terminal width, and h/l scroll through the strip.
const (
	boardColChrome = 4
	// boardCardContentFallback is used before any WindowSizeMsg arrives.
	boardCardContentFallback = 30
)

type boardModel struct {
	store      *store.Store
	projectKey string
	columns    [5][]*domain.Issue
	colCursor  int
	rowCursor  [5]int
	width      int
	height     int
	// colScrollH is the index of the leftmost visible column. Adjusted by
	// clampScroll so the selected column is always rendered when the
	// terminal can't fit all five.
	colScrollH int
	// rowScroll[i] is the index of the topmost visible card in column i.
	// Adjusted by clampScroll so the selected card stays in view.
	rowScroll    [5]int
	back         bool
	err          error
	awaitingMove bool
	// picker is the fuzzy finder overlay (Task 10). Non-nil means the
	// overlay is open and consumes keypresses; Enter snaps the board cursor
	// onto the picked card and clears the field, Esc clears without moving.
	picker         *PickerModel
	openDetailKey  *domain.IssueKey
	showMilestones bool
	// msCursor is the highlighted row in the milestone overlay. While the
	// overlay is open it captures j/k/E; esc/m/q close it.
	msCursor  int
	editorErr error
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
	// Marquee state: when the selected card's title overflows the column
	// width, it scrolls horizontally one rune per tick so the rest can be
	// read. marqueeKey tracks which issue is currently scrolling so the
	// offset resets when selection moves.
	marqueeOffset int
	marqueePause  int
	marqueeKey    *domain.IssueKey
}

type marqueeTickMsg struct{}

// marqueeTickInterval controls scroll speed. Slow enough to read, fast enough
// that long titles reveal themselves in a few seconds.
const marqueeTickInterval = 200 * time.Millisecond

func marqueeTick() tea.Cmd {
	return tea.Tick(marqueeTickInterval, func(time.Time) tea.Msg { return marqueeTickMsg{} })
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
		m.height = v.Height
		m.clampScroll()
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
		m.clampScroll()
	case marqueeTickMsg:
		selKey := m.selectedKey()
		sameSel := selKey != nil && m.marqueeKey != nil && *selKey == *m.marqueeKey
		if !sameSel {
			m.marqueeKey = selKey
			m.marqueeOffset = 0
			m.marqueePause = 3 // ~600ms pause at the start so users can read
		} else if m.marqueePause > 0 {
			m.marqueePause--
		} else {
			m.marqueeOffset++
		}
		return m, marqueeTick()
	case tea.KeyMsg:
		// While the fuzzy picker overlay is open, all key events go to it.
		// Enter snaps the board cursor onto the chosen card; Esc closes
		// without moving. Otherwise let the picker update its visible set.
		if m.picker != nil {
			next, cmd := m.picker.Update(v)
			pm := next.(PickerModel)
			m.picker = &pm
			if pm.Cancelled() {
				m.picker = nil
				return m, nil
			}
			if sel := pm.Selected(); sel != nil {
				m.picker = nil
				m.snapCursorToKey(sel.Key)
				return m, nil
			}
			return m, cmd
		}
		nm, cmd := m.handleKey(v)
		nb := nm.(boardModel)
		nb.clampScroll()
		return nb, cmd
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
		if v.editName != "" {
			newName, applyErr := applyMilestoneEditBuffer(m.store, m.projectKey, v.editName, v.tempPath)
			m.editorErr = applyErr
			// Keep an active filter pointing at the milestone across a rename.
			if applyErr == nil && m.milestoneFilter == v.editName {
				m.milestoneFilter = newName
			}
		} else if applyErr := applyMilestoneBuffer(m.store, m.projectKey, v.tempPath); applyErr != nil {
			m.editorErr = applyErr
		} else {
			m.editorErr = nil
		}
		return m, m.Init()
	case projectEditorFinishedMsg:
		if v.err != nil {
			m.editorErr = v.err
			return m, m.Init()
		}
		if applyErr := applyProjectBuffer(m.store, m.projectKey, v.tempPath); applyErr != nil {
			m.editorErr = applyErr
		} else {
			m.editorErr = nil
		}
		return m, m.Init()
	}
	return m, nil
}

func (m boardModel) handleKey(k tea.KeyMsg) (tea.Model, tea.Cmd) {
	if m.showMilestones {
		switch k.String() {
		case "j", "down":
			if m.msCursor < len(m.milestonesOrdered)-1 {
				m.msCursor++
			}
		case "k", "up":
			if m.msCursor > 0 {
				m.msCursor--
			}
		case "E":
			if m.msCursor < len(m.milestonesOrdered) {
				return m, openEditorForMilestone(m.store, m.projectKey, m.milestonesOrdered[m.msCursor].Name)
			}
		case "m", "esc", "q":
			m.showMilestones = false
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
		items := m.buildPickerItems()
		pm := NewPickerModel(items)
		m.picker = &pm
		return m, pm.Init()
	case "enter":
		sel := m.selected()
		if sel != nil {
			key := domain.IssueKey{Project: m.projectKey, Seq: sel.Seq}
			m.openDetailKey = &key
		}
	case "m":
		m.showMilestones = true
		m.msCursor = 0
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
	case "E":
		if m.milestoneFilter != "" {
			return m, openEditorForMilestone(m.store, m.projectKey, m.milestoneFilter)
		}
		return m, openEditorForProject(m.store, m.projectKey)
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

// buildPickerItems walks every card on the current board (all 5 status
// columns) and produces a flat slice of PickerItem rows for the fuzzy picker
// overlay. Labels are loaded in one bulk query so even larger projects don't
// pay an N+1 cost when opening the picker.
func (m boardModel) buildPickerItems() []PickerItem {
	var total int
	for i := 0; i < 5; i++ {
		total += len(m.columns[i])
	}
	if total == 0 {
		return nil
	}
	ids := make([]int64, 0, total)
	for i := 0; i < 5; i++ {
		for _, iss := range m.columns[i] {
			ids = append(ids, iss.ID)
		}
	}
	labelsByID := map[int64][]string{}
	if m.store != nil {
		if lbls, err := m.store.LabelsForIssues(ids); err == nil {
			labelsByID = lbls
		}
	}
	items := make([]PickerItem, 0, total)
	for i := 0; i < 5; i++ {
		statusName := string(domain.AllStatuses()[i])
		for _, iss := range m.columns[i] {
			items = append(items, PickerItem{
				Key:      fmt.Sprintf("%s-%d", m.projectKey, iss.Seq),
				Title:    iss.Title,
				Project:  m.projectKey,
				Status:   statusName,
				Priority: string(iss.Priority),
				Labels:   labelsByID[iss.ID],
			})
		}
	}
	return items
}

// snapCursorToKey moves the board cursor onto the card identified by key
// (e.g. "CLI-7"), adjusting both the column cursor and the per-column row
// cursor, and scrolling so the card is visible. A key that isn't on the
// current board (wrong project, archived, or filtered out) is a silent no-op.
func (m *boardModel) snapCursorToKey(key string) {
	parsed, err := domain.ParseIssueKey(key)
	if err != nil {
		return
	}
	if parsed.Project != m.projectKey {
		return
	}
	for col := 0; col < 5; col++ {
		for row, iss := range m.columns[col] {
			if iss.Seq == parsed.Seq {
				m.colCursor = col
				m.rowCursor[col] = row
				m.clampScroll()
				return
			}
		}
	}
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

// viewport returns the per-card content width, how many columns are visible
// (always 1 — the single column fills the terminal width), and how many lines
// are available for card content inside that column. Falls back to a roomy
// default width before any WindowSizeMsg has been received.
func (m boardModel) viewport() (cardWidth, colsToShow, rowBudget int) {
	colsToShow = 1
	cardWidth = boardCardContentFallback
	if m.width > 0 {
		cardWidth = m.width - boardColChrome
		if cardWidth < 18 {
			cardWidth = 18
		}
	}
	rowBudget = 20
	if m.height > 0 {
		// title (1) + column borders top/bottom (2) + column header (1) +
		// help bar (1) + safety margin for wrap (2).
		rowBudget = m.height - 7
		if rowBudget < 4 {
			rowBudget = 4
		}
	}
	return
}

// cardHeight returns the number of lines a card occupies when rendered.
// Cards are 2 lines normally and 3 when a milestone tag is shown.
func (m boardModel) cardHeight(issue *domain.Issue) int {
	if issue.MilestoneID != nil {
		if _, ok := m.milestonesByID[*issue.MilestoneID]; ok {
			return 3
		}
	}
	return 2
}

// clampScroll keeps colScrollH and rowScroll in a state where the current
// cursor is on screen. Reserves space for "↑ N more" / "↓ N more" overflow
// indicators so the row math doesn't have to know whether they'll be drawn.
func (m *boardModel) clampScroll() {
	_, colsToShow, rowBudget := m.viewport()

	if m.colCursor < m.colScrollH {
		m.colScrollH = m.colCursor
	}
	if m.colCursor >= m.colScrollH+colsToShow {
		m.colScrollH = m.colCursor - colsToShow + 1
	}
	if m.colScrollH < 0 {
		m.colScrollH = 0
	}
	maxScrollH := 5 - colsToShow
	if maxScrollH < 0 {
		maxScrollH = 0
	}
	if m.colScrollH > maxScrollH {
		m.colScrollH = maxScrollH
	}

	cardBudget := rowBudget - 2
	if cardBudget < 1 {
		cardBudget = 1
	}
	for i := 0; i < 5; i++ {
		cards := m.columns[i]
		if len(cards) == 0 {
			m.rowScroll[i] = 0
			continue
		}
		cursor := m.rowCursor[i]
		if cursor < 0 {
			cursor = 0
		}
		if cursor >= len(cards) {
			cursor = len(cards) - 1
		}
		if m.rowScroll[i] > cursor {
			m.rowScroll[i] = cursor
		}
		if m.rowScroll[i] < 0 {
			m.rowScroll[i] = 0
		}
		for m.rowScroll[i] < cursor {
			used := 0
			fits := false
			for r := m.rowScroll[i]; r < len(cards); r++ {
				ch := m.cardHeight(cards[r])
				if used+ch > cardBudget {
					break
				}
				used += ch
				if r == cursor {
					fits = true
					break
				}
			}
			if fits {
				break
			}
			m.rowScroll[i]++
		}
	}
}

func (m boardModel) View() string {
	// When the fuzzy picker overlay is open, take over the screen entirely.
	// A true overlay would render the picker centred over a dimmed board; a
	// full-screen takeover is simpler and feels modal in the same way fzf does.
	if m.picker != nil {
		return m.picker.View()
	}

	headers := []string{"BACKLOG", "IN-PROG", "BLOCKED", "IN-REV", "DONE"}
	cardWidth, colsToShow, rowBudget := m.viewport()
	cardBudget := rowBudget - 2
	if cardBudget < 1 {
		cardBudget = 1
	}

	visible := make([]string, 0, colsToShow)
	for offset := 0; offset < colsToShow; offset++ {
		i := m.colScrollH + offset
		if i >= 5 {
			break
		}
		cards := m.columns[i]
		cursor := m.rowCursor[i]
		if len(cards) > 0 && cursor >= len(cards) {
			cursor = len(cards) - 1
		}

		scroll := m.rowScroll[i]
		if scroll > 0 && scroll > cursor {
			scroll = cursor
		}
		if scroll < 0 {
			scroll = 0
		}

		var b strings.Builder
		fmt.Fprintf(&b, "%s (%d)\n", headers[i], len(cards))
		if scroll > 0 {
			fmt.Fprintf(&b, "%s\n", StyleMuted.Render(fmt.Sprintf("↑ %d more", scroll)))
		}

		used := 0
		lastVisible := scroll - 1
		for r := scroll; r < len(cards); r++ {
			issue := cards[r]
			ch := m.cardHeight(issue)
			if used+ch > cardBudget {
				break
			}
			used += ch
			lastVisible = r
			titleWidth := cardWidth - 2
			isSelected := i == m.colCursor && r == cursor
			var titleLine string
			if isSelected && m.marqueeKey != nil && m.marqueeKey.Seq == issue.Seq {
				titleLine = renderMarquee(issue.Title, titleWidth, m.marqueeOffset)
			} else {
				titleLine = truncate(issue.Title, titleWidth)
			}
			milestoneLine := ""
			if issue.MilestoneID != nil {
				if name, ok := m.milestonesByID[*issue.MilestoneID]; ok {
					milestoneLine = "\n  " + StyleMuted.Render("⟜ "+truncate(name, cardWidth-4))
				}
			}
			card := fmt.Sprintf("%s-%d %s\n  %s%s",
				m.projectKey, issue.Seq, PriorityDot(string(issue.Priority)),
				titleLine, milestoneLine)
			if i == m.colCursor && r == cursor {
				card = StyleSelected.Render(card)
			}
			b.WriteString(card + "\n")
		}
		if lastVisible+1 < len(cards) {
			remaining := len(cards) - (lastVisible + 1)
			fmt.Fprintf(&b, "%s", StyleMuted.Render(fmt.Sprintf("↓ %d more", remaining)))
		}

		visible = append(visible, StyleColumn.Width(cardWidth).Render(b.String()))
	}
	body := lipgloss.JoinHorizontal(lipgloss.Top, visible...)

	helpText := "hjkl move  enter detail  e edit  E proj/ms  n new  N ms+  t tag-ms  Space mv  a archive  M ms-filter  / find  r refresh  q quit"
	helpText = fmt.Sprintf("col %d/5  | %s", m.colCursor+1, helpText)
	if m.milestoneFilter != "" {
		helpText = fmt.Sprintf("milestone: %s  | %s", m.milestoneFilter, helpText)
	}
	if m.editorErr != nil {
		helpText = "ERR: " + m.editorErr.Error() + " | " + helpText
	}
	help := StyleStatusBar.Render(helpText)
	base := StyleTitle.Render(fmt.Sprintf("cliban — %s", m.projectKey)) + "\n" + body + "\n" + help

	if m.showMilestones {
		return base + "\n" + renderMilestoneOverlay(m.store, m.projectKey, m.msCursor)
	}
	return base
}

// renderMarquee returns a width-wide window over a cyclic "title<sep>title<sep>…"
// strip, advanced by offset runes. Titles that already fit are returned as-is.
func renderMarquee(title string, width, offset int) string {
	if width <= 0 {
		return ""
	}
	runes := []rune(title)
	if len(runes) <= width {
		return title
	}
	sep := []rune("   •   ")
	cycle := make([]rune, 0, len(runes)+len(sep))
	cycle = append(cycle, runes...)
	cycle = append(cycle, sep...)
	n := len(cycle)
	offset = ((offset % n) + n) % n
	out := make([]rune, width)
	for i := 0; i < width; i++ {
		out[i] = cycle[(offset+i)%n]
	}
	return string(out)
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
