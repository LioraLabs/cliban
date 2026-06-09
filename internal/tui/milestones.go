package tui

import (
	"fmt"
	"strings"

	"github.com/alex/cliban/internal/store"
)

func renderMilestoneOverlay(s *store.Store, projectKey string, cursor int) string {
	ms, err := s.ListMilestones(projectKey, "")
	if err != nil {
		return "milestones: " + err.Error()
	}
	if len(ms) == 0 {
		return StyleMuted.Render("(no milestones)")
	}
	var sb strings.Builder
	sb.WriteString(StyleTitle.Render("milestones") + "\n")
	for i, m := range ms {
		target := "-"
		if m.TargetDate != nil {
			target = m.TargetDate.Format("2006-01-02")
		}
		line := fmt.Sprintf("%-15s %-10s %s", m.Name, m.Status, target)
		if i == cursor {
			line = StyleSelected.Render("▸ " + line)
		} else {
			line = "  " + line
		}
		sb.WriteString(line + "\n")
	}
	sb.WriteString(StyleMuted.Render("j/k move  E edit  esc close") + "\n")
	return sb.String()
}
