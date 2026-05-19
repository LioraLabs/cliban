package cli

import (
	"github.com/alex/cliban/internal/tui"
	tea "github.com/charmbracelet/bubbletea"
)

func init() {
	RunTUI = func() error {
		s, err := openStore()
		if err != nil {
			return err
		}
		defer s.Close()
		p := tea.NewProgram(tui.NewModel(s), tea.WithAltScreen())
		_, err = p.Run()
		return err
	}
}
