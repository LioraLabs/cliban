package tui

import (
	"fmt"
	"os"
	"time"

	"github.com/alex/cliban/internal/issuebuf"
	"github.com/alex/cliban/internal/store"
	tea "github.com/charmbracelet/bubbletea"
)

type milestoneEditorFinishedMsg struct {
	tempPath string
	err      error
}

func openEditorForNewMilestone(projectKey string) tea.Cmd {
	bf := issuebuf.MilestoneBuffer{
		Header: fmt.Sprintf("# Creating milestone in %s — lines above the first '---' are ignored.\n# Status: open | completed | cancelled\n# Target date: YYYY-MM-DD (or leave empty)", projectKey),
		Status: "open",
	}
	path, err := issuebuf.WriteTempBuffer(fmt.Sprintf("cliban-milestone-%s", projectKey), bf.Serialize())
	if err != nil {
		return func() tea.Msg { return milestoneEditorFinishedMsg{err: err} }
	}
	return tea.ExecProcess(execEditorCmd(path), func(err error) tea.Msg {
		return milestoneEditorFinishedMsg{tempPath: path, err: err}
	})
}

func applyMilestoneBuffer(s *store.Store, projectKey, path string) error {
	data, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	bf, err := issuebuf.ParseMilestoneBuffer(string(data))
	if err != nil {
		return fmt.Errorf("buffer parse (preserved at %s): %w", path, err)
	}
	var target *time.Time
	if bf.Target != "" {
		t, err := time.Parse("2006-01-02", bf.Target)
		if err != nil {
			return fmt.Errorf("invalid target %q (want YYYY-MM-DD)", bf.Target)
		}
		target = &t
	}
	_, err = s.CreateMilestone(projectKey, bf.Name, bf.Description, target)
	return err
}
