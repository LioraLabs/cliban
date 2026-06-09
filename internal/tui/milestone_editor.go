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
	// editName is the original name of the milestone being edited; empty
	// means the buffer creates a new milestone.
	editName string
	err      error
}

func openEditorForMilestone(s *store.Store, projectKey, name string) tea.Cmd {
	ms, err := s.GetMilestone(projectKey, name)
	if err != nil {
		return func() tea.Msg { return milestoneEditorFinishedMsg{err: err} }
	}
	target := ""
	if ms.TargetDate != nil {
		target = ms.TargetDate.Format("2006-01-02")
	}
	bf := issuebuf.MilestoneBuffer{
		Header:      fmt.Sprintf("# Editing milestone %s in %s — lines above the first '---' are ignored.\n# Status: open | completed | cancelled\n# Target date: YYYY-MM-DD (empty clears it)", name, projectKey),
		Name:        ms.Name,
		Target:      target,
		Status:      string(ms.Status),
		Description: ms.Description,
	}
	path, err := issuebuf.WriteTempBuffer(fmt.Sprintf("cliban-milestone-%s", projectKey), bf.Serialize())
	if err != nil {
		return func() tea.Msg { return milestoneEditorFinishedMsg{err: err} }
	}
	return tea.ExecProcess(execEditorCmd(path), func(err error) tea.Msg {
		return milestoneEditorFinishedMsg{tempPath: path, editName: name, err: err}
	})
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

// applyMilestoneEditBuffer parses the edited buffer and applies it to the
// existing milestone originalName. The buffer's name field may differ from
// originalName, which renames the milestone. An empty target clears the date.
// Returns the milestone's name after the edit (new name when renamed).
func applyMilestoneEditBuffer(s *store.Store, projectKey, originalName, path string) (string, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return originalName, err
	}
	bf, err := issuebuf.ParseMilestoneBuffer(string(data))
	if err != nil {
		return originalName, fmt.Errorf("buffer parse (preserved at %s): %w", path, err)
	}
	params := store.UpdateMilestoneParams{Description: &bf.Description}
	if bf.Name != originalName {
		params.NewName = &bf.Name
	}
	if bf.Status != "" {
		params.Status = &bf.Status
	}
	if bf.Target == "" {
		params.ClearTarget = true
	} else {
		t, err := time.Parse("2006-01-02", bf.Target)
		if err != nil {
			return originalName, fmt.Errorf("invalid target %q (want YYYY-MM-DD)", bf.Target)
		}
		params.TargetDate = &t
	}
	if err := s.UpdateMilestone(projectKey, originalName, params); err != nil {
		return originalName, err
	}
	return bf.Name, nil
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
