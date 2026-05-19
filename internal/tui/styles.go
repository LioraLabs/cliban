package tui

import "github.com/charmbracelet/lipgloss"

var (
	StyleTitle     = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("12"))
	StyleSelected  = lipgloss.NewStyle().Background(lipgloss.Color("236")).Foreground(lipgloss.Color("15"))
	StyleMuted     = lipgloss.NewStyle().Foreground(lipgloss.Color("244"))
	StyleColumn    = lipgloss.NewStyle().Border(lipgloss.RoundedBorder()).Padding(0, 1)
	StyleStatusBar = lipgloss.NewStyle().Background(lipgloss.Color("237")).Foreground(lipgloss.Color("252")).Padding(0, 1)
)

func PriorityDot(p string) string {
	switch p {
	case "urgent":
		return lipgloss.NewStyle().Foreground(lipgloss.Color("196")).Render("●U")
	case "high":
		return lipgloss.NewStyle().Foreground(lipgloss.Color("208")).Render("●H")
	case "medium":
		return lipgloss.NewStyle().Foreground(lipgloss.Color("226")).Render("●M")
	case "low":
		return lipgloss.NewStyle().Foreground(lipgloss.Color("33")).Render("●L")
	default:
		return StyleMuted.Render("·")
	}
}
