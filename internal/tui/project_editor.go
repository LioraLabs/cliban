package tui

import (
	"fmt"
	"os"

	"github.com/alex/cliban/internal/issuebuf"
	"github.com/alex/cliban/internal/store"
	tea "github.com/charmbracelet/bubbletea"
)

type projectEditorFinishedMsg struct {
	tempPath   string
	projectKey string
	err        error
}

func openEditorForProject(s *store.Store, projectKey string) tea.Cmd {
	p, err := s.GetProjectByKey(projectKey)
	if err != nil {
		return func() tea.Msg { return projectEditorFinishedMsg{err: err} }
	}
	bf := issuebuf.ProjectBuffer{
		Header:      fmt.Sprintf("# Editing project %s — lines above the first '---' are ignored.\n# The project key is immutable; rename via 'name'.", projectKey),
		Name:        p.Name,
		Description: p.Description,
	}
	path, err := issuebuf.WriteTempBuffer(fmt.Sprintf("cliban-project-%s", projectKey), bf.Serialize())
	if err != nil {
		return func() tea.Msg { return projectEditorFinishedMsg{err: err} }
	}
	return tea.ExecProcess(execEditorCmd(path), func(err error) tea.Msg {
		return projectEditorFinishedMsg{tempPath: path, projectKey: projectKey, err: err}
	})
}

func applyProjectBuffer(s *store.Store, projectKey, path string) error {
	data, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	bf, err := issuebuf.ParseProjectBuffer(string(data))
	if err != nil {
		return fmt.Errorf("buffer parse (preserved at %s): %w", path, err)
	}
	return s.UpdateProject(projectKey, bf.Name, bf.Description)
}
