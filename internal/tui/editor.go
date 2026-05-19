package tui

import (
	"fmt"
	"os"
	"os/exec"

	"github.com/alex/cliban/internal/domain"
	"github.com/alex/cliban/internal/issuebuf"
	"github.com/alex/cliban/internal/store"
	tea "github.com/charmbracelet/bubbletea"
)

type editorFinishedMsg struct {
	tempPath string
	editKey  *domain.IssueKey // nil for new
	err      error
}

func openEditorForIssue(s *store.Store, k domain.IssueKey) tea.Cmd {
	issue, err := s.GetIssueByKey(k)
	if err != nil {
		return func() tea.Msg { return editorFinishedMsg{err: err} }
	}
	bf := issuebuf.IssueBuffer{
		Header: fmt.Sprintf("# Editing %s — lines above the first '---' are ignored.\n# Statuses:   backlog | in-progress | blocked | in-review | done\n# Priorities: none | low | medium | high | urgent\n# Set milestone or parent to '' to clear.", k),
		Title:       issue.Title,
		Status:      string(issue.Status),
		Priority:    string(issue.Priority),
		Description: issue.Description,
	}
	if issue.MilestoneID != nil {
		ms, _ := s.ListMilestones(k.Project, "")
		for _, m := range ms {
			if m.ID == *issue.MilestoneID {
				bf.Milestone = m.Name
				break
			}
		}
	}
	if issue.ParentID != nil {
		parent, err := s.GetIssueByID(*issue.ParentID)
		if err == nil && parent != nil {
			bf.Parent = fmt.Sprintf("%s-%d", k.Project, parent.Seq)
		}
	}
	path, err := issuebuf.WriteTempBuffer(fmt.Sprintf("cliban-issue-%s-%d", k.Project, k.Seq), bf.Serialize())
	if err != nil {
		return func() tea.Msg { return editorFinishedMsg{err: err} }
	}
	keyCopy := k
	return tea.ExecProcess(execEditorCmd(path), func(err error) tea.Msg {
		return editorFinishedMsg{tempPath: path, editKey: &keyCopy, err: err}
	})
}

func openEditorForNew(s *store.Store, projectKey string, defaultStatus domain.Status) tea.Cmd {
	bf := issuebuf.IssueBuffer{
		Header:   fmt.Sprintf("# Creating issue in %s — lines above the first '---' are ignored.\n# Statuses:   backlog | in-progress | blocked | in-review | done\n# Priorities: none | low | medium | high | urgent", projectKey),
		Status:   string(defaultStatus),
		Priority: string(domain.PriorityNone),
	}
	path, err := issuebuf.WriteTempBuffer(fmt.Sprintf("cliban-new-%s", projectKey), bf.Serialize())
	if err != nil {
		return func() tea.Msg { return editorFinishedMsg{err: err} }
	}
	return tea.ExecProcess(execEditorCmd(path), func(err error) tea.Msg {
		return editorFinishedMsg{tempPath: path, editKey: nil, err: err}
	})
}

func execEditorCmd(path string) *exec.Cmd {
	editor := issuebuf.ResolveEditor()
	cmd := exec.Command("sh", "-c", fmt.Sprintf("%s %q", editor, path))
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	return cmd
}

// applyBufferToStore reads the temp file, parses, and applies the changes.
func applyBufferToStore(s *store.Store, projectKey string, current *domain.IssueKey, path string) error {
	data, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	bf, err := issuebuf.ParseIssueBuffer(string(data))
	if err != nil {
		return fmt.Errorf("buffer parse (preserved at %s): %w", path, err)
	}
	if current == nil {
		params := store.CreateIssueParams{
			ProjectKey:    projectKey,
			Title:         bf.Title,
			Description:   bf.Description,
			MilestoneName: bf.Milestone,
		}
		if bf.Status != "" {
			params.Status = domain.Status(bf.Status)
		}
		if bf.Priority != "" {
			params.Priority = domain.Priority(bf.Priority)
		}
		if bf.Parent != "" {
			pk, err := domain.ParseIssueKey(bf.Parent)
			if err != nil {
				return err
			}
			params.ParentKey = &pk
		}
		_, err := s.CreateIssue(params)
		return err
	}
	cur, err := s.GetIssueByKey(*current)
	if err != nil {
		return err
	}
	up := store.UpdateIssueParams{}
	if bf.Title != cur.Title {
		up.Title = &bf.Title
	}
	if bf.Description != cur.Description {
		up.Description = &bf.Description
	}
	if bf.Priority != "" && bf.Priority != string(cur.Priority) {
		p := domain.Priority(bf.Priority)
		up.Priority = &p
	}
	if bf.Status != "" && bf.Status != string(cur.Status) {
		st, err := domain.ParseStatus(bf.Status)
		if err != nil {
			return err
		}
		if err := s.MoveIssue(*current, st); err != nil {
			return err
		}
	}
	if bf.Milestone == "" {
		up.ClearMilestone = true
	} else {
		up.Milestone = &bf.Milestone
	}
	if bf.Parent == "" {
		up.ClearParent = true
	} else {
		pk, err := domain.ParseIssueKey(bf.Parent)
		if err != nil {
			return err
		}
		up.Parent = &pk
	}
	return s.UpdateIssue(*current, up)
}
