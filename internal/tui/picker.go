package tui

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/sahilm/fuzzy"
)

// PickerItem is one row in the fuzzy picker. The `candidate` field is the
// concatenation that fuzzy.Find scores against on every keystroke; it is
// computed once at NewPickerModel time so re-filtering stays allocation-free.
type PickerItem struct {
	Key       string
	Title     string
	Project   string
	Status    string
	Priority  string
	Labels    []string
	candidate string // "KEY title project labels" — computed once
}

func (p *PickerItem) computeCandidate() {
	p.candidate = fmt.Sprintf("%s %s %s %s", p.Key, p.Title, p.Project, strings.Join(p.Labels, " "))
}

// PickerModel is the Bubble Tea model behind `cliban fff`. It is a snapshot
// picker: `items` is loaded once by the caller (via search.Search), and every
// keystroke re-ranks that fixed slice in-memory with sahilm/fuzzy. We never
// re-query the store while the picker is open.
type PickerModel struct {
	items      []PickerItem
	visible    []PickerItem
	visiblePos [][]int // matched rune indices for each visible item; parallel to visible
	input      textinput.Model
	cursor     int
	selected   *PickerItem // set on Enter when there is a current row
	quitted    bool        // user pressed Esc or Ctrl-C
	width      int
	height     int
}

// NewPickerModel builds a model from items, precomputes candidate strings,
// focuses the input, and primes the visible slice with all items (empty
// query). Callers can then drive the model through tea.NewProgram.
func NewPickerModel(items []PickerItem) PickerModel {
	for i := range items {
		items[i].computeCandidate()
	}
	ti := textinput.New()
	ti.Placeholder = "fuzzy query…"
	ti.Prompt = "> "
	ti.Focus()
	m := PickerModel{items: items, input: ti}
	return m.setQuery("")
}

// setQuery is the single source of truth for what's visible. It is called
// once from NewPickerModel and once per keystroke from Update. Empty/trimmed
// query → all items; otherwise re-rank via fuzzy.Find (already sorted desc).
//
// Ranking divergence: this filter scores the per-item `candidate` (a
// concatenation of KEY/title/project/labels) in one pass, NOT through the
// weighted multi-field scoring in internal/search.Search. The result is
// faster per keystroke (one fuzzy.Find call per items slice, no per-field
// loop) but can rank differently from `cliban issue ls --search` for the
// same query — description text isn't in the candidate, and title isn't
// weighted above labels. Acceptable for the snapshot-at-open picker model.
func (m PickerModel) setQuery(q string) PickerModel {
	q = strings.TrimSpace(q)
	if q == "" {
		m.visible = append(m.visible[:0], m.items...)
		m.visiblePos = m.visiblePos[:0]
		for range m.items {
			m.visiblePos = append(m.visiblePos, nil)
		}
	} else {
		cands := make([]string, len(m.items))
		for i, it := range m.items {
			cands[i] = it.candidate
		}
		hits := fuzzy.Find(q, cands)
		m.visible = m.visible[:0]
		m.visiblePos = m.visiblePos[:0]
		for _, h := range hits {
			m.visible = append(m.visible, m.items[h.Index])
			m.visiblePos = append(m.visiblePos, h.MatchedIndexes)
		}
	}
	if m.cursor >= len(m.visible) {
		m.cursor = 0
	}
	return m
}

// WithInitialQuery seeds the textinput with q and pre-filters the visible
// set so the first frame already reflects the caller's query. Useful when
// `cliban fff QUERY` is invoked with an arg — the user sees the narrowed
// list immediately and can keep typing to refine.
func (m PickerModel) WithInitialQuery(q string) PickerModel {
	m.input.SetValue(q)
	m.input.CursorEnd()
	return m.setQuery(q)
}

// Selected returns the item the user picked with Enter, or nil if the picker
// quit without a selection (Esc / Ctrl-C / empty visible list).
func (m PickerModel) Selected() *PickerItem { return m.selected }

// Cancelled is true when the user explicitly quit (Esc or Ctrl-C) without
// picking. The caller treats this as a non-zero exit, distinct from "picker
// closed normally with a selection".
func (m PickerModel) Cancelled() bool { return m.quitted && m.selected == nil }

func (m PickerModel) Init() tea.Cmd { return textinput.Blink }

func (m PickerModel) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width, m.height = msg.Width, msg.Height
		return m, nil
	case tea.KeyMsg:
		switch msg.String() {
		case "esc":
			m.quitted = true
			return m, tea.Quit
		case "ctrl+c":
			m.quitted = true
			return m, tea.Quit
		case "enter":
			if len(m.visible) > 0 {
				v := m.visible[m.cursor]
				m.selected = &v
			}
			m.quitted = true
			return m, tea.Quit
		case "up", "ctrl+p":
			if m.cursor > 0 {
				m.cursor--
			}
			return m, nil
		case "down", "ctrl+n":
			if m.cursor < len(m.visible)-1 {
				m.cursor++
			}
			return m, nil
		default:
			var cmd tea.Cmd
			m.input, cmd = m.input.Update(msg)
			m = m.setQuery(m.input.Value())
			return m, cmd
		}
	}
	return m, nil
}

var (
	pickerCursorMark   = lipgloss.NewStyle().Foreground(lipgloss.Color("12")).Bold(true)
	pickerSelectedRow  = lipgloss.NewStyle().Bold(true)
	pickerProjectStyle = lipgloss.NewStyle().Foreground(lipgloss.Color("33"))
	pickerLabelStyle   = lipgloss.NewStyle().Foreground(lipgloss.Color("244"))
	pickerCountStyle   = lipgloss.NewStyle().Foreground(lipgloss.Color("244"))
	pickerHelpStyle    = lipgloss.NewStyle().Faint(true)
)

func (m PickerModel) View() string {
	var b strings.Builder
	b.WriteString(m.input.View())
	b.WriteString("\n")
	b.WriteString(pickerCountStyle.Render(fmt.Sprintf("%d / %d", len(m.visible), len(m.items))))
	b.WriteString("\n")

	for i, it := range m.visible {
		prefix := "  "
		if i == m.cursor {
			prefix = pickerCursorMark.Render("▌ ")
		}
		row := fmt.Sprintf("%s  %s", it.Key, it.Title)
		if it.Project != "" {
			row += "  " + pickerProjectStyle.Render("·"+it.Project)
		}
		if len(it.Labels) > 0 {
			row += "  " + pickerLabelStyle.Render("·"+strings.Join(it.Labels, " ·"))
		}
		if i == m.cursor {
			row = pickerSelectedRow.Render(row)
		}
		b.WriteString(prefix)
		b.WriteString(row)
		b.WriteString("\n")
	}
	b.WriteString(pickerHelpStyle.Render("↑/↓ or ctrl+p/ctrl+n move  enter pick  esc cancel"))
	return b.String()
}
