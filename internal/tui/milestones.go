package tui

import (
	"fmt"
	"strings"

	"github.com/alex/cliban/internal/store"
)

func renderMilestoneOverlay(s *store.Store, projectKey string) string {
	ms, err := s.ListMilestones(projectKey, "")
	if err != nil {
		return "milestones: " + err.Error()
	}
	if len(ms) == 0 {
		return StyleMuted.Render("(no milestones)")
	}
	var sb strings.Builder
	sb.WriteString(StyleTitle.Render("milestones") + "\n")
	for _, m := range ms {
		target := "-"
		if m.TargetDate != nil {
			target = m.TargetDate.Format("2006-01-02")
		}
		fmt.Fprintf(&sb, "  %-15s %-10s %s\n", m.Name, m.Status, target)
	}
	return sb.String()
}
