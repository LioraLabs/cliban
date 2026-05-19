package cli

import (
	"fmt"
	"io"
	"os"
	"strings"

	"github.com/alex/cliban/internal/domain"
	"github.com/alex/cliban/internal/store"
	"github.com/spf13/cobra"
)

func issueCmd() *cobra.Command {
	c := &cobra.Command{Use: "issue", Short: "Manage issues"}
	c.AddCommand(issueAddCmd(), issueListCmd(), issueShowCmd(), issueEditCmd(), issueMvCmd(), issueRmCmd())
	return c
}

// readMaybeStdin returns the literal value if not "-", otherwise reads stdin to EOF.
func readMaybeStdin(v string) (string, error) {
	if v != "-" {
		return v, nil
	}
	data, err := io.ReadAll(os.Stdin)
	if err != nil {
		return "", err
	}
	return string(data), nil
}

func issueAddCmd() *cobra.Command {
	var project, title, desc, parent, milestone, priority, status string
	var asJSON, noEditor bool
	c := &cobra.Command{
		Use:   "add",
		Short: "Add an issue (no --title and no --description opens $EDITOR in Task 15)",
		RunE: func(cmd *cobra.Command, args []string) error {
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()

			// Editor path: only triggered when no content flags supplied.
			contentless := title == "" && desc == ""
			if contentless {
				if EditorDisabled(noEditor) {
					return fmt.Errorf("%w: --title required when --no-editor or $CLIBAN_NO_EDITOR is set", store.ErrValidation)
				}
				if err := RequireTTY(); err != nil {
					return fmt.Errorf("%w: %v", store.ErrValidation, err)
				}
				header := fmt.Sprintf("# Creating issue in %s — lines above the first '---' are ignored.\n# Statuses:   backlog | in-progress | blocked | in-review | done\n# Priorities: none | low | medium | high | urgent",
					strings.ToUpper(project))
				blank := IssueBuffer{
					Header:   header,
					Title:    "",
					Status:   string(domain.StatusBacklog),
					Priority: string(domain.PriorityNone),
				}.Serialize()
				path, err := WriteTempBuffer("cliban-new", blank)
				if err != nil {
					return err
				}
				if err := RunEditor(path); err != nil {
					return err
				}
				data, err := os.ReadFile(path)
				if err != nil {
					return err
				}
				bf, err := ParseIssueBuffer(string(data))
				if err != nil {
					return fmt.Errorf("buffer parse (file preserved at %s): %w", path, err)
				}
				params := store.CreateIssueParams{
					ProjectKey:    strings.ToUpper(project),
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
					k, err := domain.ParseIssueKey(bf.Parent)
					if err != nil {
						return err
					}
					params.ParentKey = &k
				}
				issue, err := s.CreateIssue(params)
				if err != nil {
					return err
				}
				if asJSON {
					return WriteIssueJSON(cmd.OutOrStdout(), params.ProjectKey, issue)
				}
				fmt.Fprintf(cmd.OutOrStdout(), "created %s-%d: %s\n", params.ProjectKey, issue.Seq, issue.Title)
				return nil
			}

			descContent, err := readMaybeStdin(desc)
			if err != nil {
				return err
			}

			params := store.CreateIssueParams{
				ProjectKey:    strings.ToUpper(project),
				Title:         title,
				Description:   descContent,
				MilestoneName: milestone,
			}
			if priority != "" {
				p, err := domain.ParsePriority(priority)
				if err != nil {
					return fmt.Errorf("%w: %v", store.ErrValidation, err)
				}
				params.Priority = p
			}
			if status != "" {
				st, err := domain.ParseStatus(status)
				if err != nil {
					return fmt.Errorf("%w: %v", store.ErrValidation, err)
				}
				params.Status = st
			}
			if parent != "" {
				k, err := domain.ParseIssueKey(parent)
				if err != nil {
					return fmt.Errorf("%w: %v", store.ErrValidation, err)
				}
				params.ParentKey = &k
			}
			issue, err := s.CreateIssue(params)
			if err != nil {
				return err
			}
			if asJSON {
				return WriteIssueJSON(cmd.OutOrStdout(), params.ProjectKey, issue)
			}
			fmt.Fprintf(cmd.OutOrStdout(), "created %s-%d: %s\n", params.ProjectKey, issue.Seq, issue.Title)
			return nil
		},
	}
	c.Flags().StringVar(&project, "project", "", "project key (required)")
	c.Flags().StringVar(&title, "title", "", "issue title")
	c.Flags().StringVar(&desc, "description", "", "description (use '-' to read from stdin)")
	c.Flags().StringVar(&parent, "parent", "", "parent issue key (sub-issue)")
	c.Flags().StringVar(&milestone, "milestone", "", "milestone name")
	c.Flags().StringVar(&priority, "priority", "", "priority")
	c.Flags().StringVar(&status, "status", "", "status")
	c.Flags().BoolVar(&asJSON, "json", false, "JSON output")
	c.Flags().BoolVar(&noEditor, "no-editor", false, "fail rather than opening $EDITOR")
	_ = c.MarkFlagRequired("project")
	return c
}

func issueListCmd() *cobra.Command {
	var project, status, priority, milestone, parent string
	var noSubs, asJSON bool
	c := &cobra.Command{
		Use:   "ls",
		Short: "List issues",
		RunE: func(cmd *cobra.Command, args []string) error {
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()
			f := store.ListIssuesFilter{
				ProjectKey:    strings.ToUpper(project),
				MilestoneName: milestone,
				NoSubs:        noSubs,
			}
			if status != "" {
				st, err := domain.ParseStatus(status)
				if err != nil {
					return err
				}
				f.Status = st
			}
			if priority != "" {
				pr, err := domain.ParsePriority(priority)
				if err != nil {
					return err
				}
				f.Priority = pr
			}
			if parent != "" {
				k, err := domain.ParseIssueKey(parent)
				if err != nil {
					return err
				}
				f.ParentKey = &k
			}
			issues, err := s.ListIssues(f)
			if err != nil {
				return err
			}
			projects := projectKeysByID(s)
			if asJSON {
				for _, i := range issues {
					pk := projects[i.ProjectID]
					if err := WriteIssueJSON(cmd.OutOrStdout(), pk, i); err != nil {
						return err
					}
				}
				return nil
			}
			rows := []ListIssueRow{}
			for _, i := range issues {
				pk := projects[i.ProjectID]
				rows = append(rows, ListIssueRow{
					Key:      fmt.Sprintf("%s-%d", pk, i.Seq),
					Title:    i.Title,
					Status:   string(i.Status),
					Priority: string(i.Priority),
				})
			}
			WriteIssueTable(cmd.OutOrStdout(), rows)
			return nil
		},
	}
	c.Flags().StringVar(&project, "project", "", "project key filter")
	c.Flags().StringVar(&status, "status", "", "status filter")
	c.Flags().StringVar(&priority, "priority", "", "priority filter")
	c.Flags().StringVar(&milestone, "milestone", "", "milestone filter")
	c.Flags().StringVar(&parent, "parent", "", "list sub-issues of this parent key")
	c.Flags().BoolVar(&noSubs, "no-subs", false, "exclude sub-issues")
	c.Flags().BoolVar(&asJSON, "json", false, "NDJSON output")
	return c
}

func projectKeysByID(s *store.Store) map[int64]string {
	out := map[int64]string{}
	ps, err := s.ListProjects(true)
	if err != nil {
		return out
	}
	for _, p := range ps {
		out[p.ID] = p.Key
	}
	return out
}

func issueShowCmd() *cobra.Command {
	var asJSON bool
	c := &cobra.Command{
		Use:   "show <KEY>",
		Short: "Show an issue",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()
			k, err := domain.ParseIssueKey(args[0])
			if err != nil {
				return err
			}
			issue, err := s.GetIssueByKey(k)
			if err != nil {
				return err
			}
			if asJSON {
				return WriteIssueJSON(cmd.OutOrStdout(), k.Project, issue)
			}
			fmt.Fprintf(cmd.OutOrStdout(), "%s — %s\nstatus:   %s\npriority: %s\n\n%s\n",
				k, issue.Title, issue.Status, issue.Priority, issue.Description)
			return nil
		},
	}
	c.Flags().BoolVar(&asJSON, "json", false, "JSON output")
	return c
}

func issueEditCmd() *cobra.Command {
	var title, desc, priority, milestone, parent string
	var clearMilestone, clearParent, force, noEditor bool
	c := &cobra.Command{
		Use:   "edit <KEY>",
		Short: "Edit an issue (no edit flags opens $EDITOR in Task 15)",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()
			k, err := domain.ParseIssueKey(args[0])
			if err != nil {
				return err
			}
			params := store.UpdateIssueParams{}
			if cmd.Flags().Changed("title") {
				params.Title = &title
			}
			if cmd.Flags().Changed("description") {
				v, err := readMaybeStdin(desc)
				if err != nil {
					return err
				}
				params.Description = &v
			}
			if cmd.Flags().Changed("priority") {
				p, err := domain.ParsePriority(priority)
				if err != nil {
					return err
				}
				params.Priority = &p
			}
			if clearMilestone {
				params.ClearMilestone = true
			} else if cmd.Flags().Changed("milestone") {
				params.Milestone = &milestone
			}
			if clearParent {
				params.ClearParent = true
			} else if cmd.Flags().Changed("parent") {
				pk, err := domain.ParseIssueKey(parent)
				if err != nil {
					return err
				}
				params.Parent = &pk
			}
			noChanges := !cmd.Flags().Changed("title") && !cmd.Flags().Changed("description") &&
				!cmd.Flags().Changed("priority") && !cmd.Flags().Changed("milestone") &&
				!cmd.Flags().Changed("parent") && !clearMilestone && !clearParent
			if noChanges && !force {
				if EditorDisabled(noEditor) {
					return fmt.Errorf("%w: no edits requested and editor disabled", store.ErrValidation)
				}
				if err := RequireTTY(); err != nil {
					return fmt.Errorf("%w: %v", store.ErrValidation, err)
				}
				cur, err := s.GetIssueByKey(k)
				if err != nil {
					return err
				}
				bf := IssueBuffer{
					Header:      fmt.Sprintf("# Editing %s — lines above the first '---' are ignored.\n# Statuses:   backlog | in-progress | blocked | in-review | done\n# Priorities: none | low | medium | high | urgent\n# Set milestone or parent to '' to clear.", k),
					Title:       cur.Title,
					Status:      string(cur.Status),
					Priority:    string(cur.Priority),
					Description: cur.Description,
				}
				if cur.MilestoneID != nil {
					ms, _ := s.ListMilestones(k.Project, "")
					for _, m := range ms {
						if m.ID == *cur.MilestoneID {
							bf.Milestone = m.Name
							break
						}
					}
				}
				if cur.ParentID != nil {
					parent, err := s.GetIssueByID(*cur.ParentID)
					if err == nil && parent != nil {
						bf.Parent = fmt.Sprintf("%s-%d", k.Project, parent.Seq)
					}
				}
				path, err := WriteTempBuffer(fmt.Sprintf("cliban-issue-%s-%d", k.Project, k.Seq), bf.Serialize())
				if err != nil {
					return err
				}
				if err := RunEditor(path); err != nil {
					return err
				}
				data, err := os.ReadFile(path)
				if err != nil {
					return err
				}
				next, err := ParseIssueBuffer(string(data))
				if err != nil {
					return fmt.Errorf("buffer parse (file preserved at %s): %w", path, err)
				}
				up := store.UpdateIssueParams{}
				if next.Title != cur.Title {
					up.Title = &next.Title
				}
				if next.Description != cur.Description {
					up.Description = &next.Description
				}
				if next.Priority != "" && next.Priority != string(cur.Priority) {
					pri := domain.Priority(next.Priority)
					up.Priority = &pri
				}
				switch {
				case next.Milestone == "" && bf.Milestone != "":
					up.ClearMilestone = true
				case next.Milestone != bf.Milestone:
					up.Milestone = &next.Milestone
				}
				switch {
				case next.Parent == "" && bf.Parent != "":
					up.ClearParent = true
				case next.Parent != bf.Parent && next.Parent != "":
					pk, err := domain.ParseIssueKey(next.Parent)
					if err != nil {
						return err
					}
					up.Parent = &pk
				}
				if next.Status != "" && next.Status != string(cur.Status) {
					st, err := domain.ParseStatus(next.Status)
					if err != nil {
						return err
					}
					if err := s.MoveIssue(k, st); err != nil {
						return err
					}
				}
				return s.UpdateIssue(k, up)
			}
			return s.UpdateIssue(k, params)
		},
	}
	c.Flags().StringVar(&title, "title", "", "new title")
	c.Flags().StringVar(&desc, "description", "", "new description (use '-' for stdin)")
	c.Flags().StringVar(&priority, "priority", "", "new priority")
	c.Flags().StringVar(&milestone, "milestone", "", "new milestone")
	c.Flags().BoolVar(&clearMilestone, "clear-milestone", false, "clear milestone")
	c.Flags().StringVar(&parent, "parent", "", "new parent key")
	c.Flags().BoolVar(&clearParent, "clear-parent", false, "clear parent")
	c.Flags().BoolVarP(&force, "edit", "e", false, "force editor open (no-op until Task 15)")
	c.Flags().BoolVar(&noEditor, "no-editor", false, "never open $EDITOR")
	return c
}

func issueMvCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "mv <KEY> <STATUS>",
		Short: "Move an issue to a new status",
		Args:  cobra.ExactArgs(2),
		RunE: func(cmd *cobra.Command, args []string) error {
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()
			k, err := domain.ParseIssueKey(args[0])
			if err != nil {
				return err
			}
			st, err := domain.ParseStatus(args[1])
			if err != nil {
				return err
			}
			return s.MoveIssue(k, st)
		},
	}
}

func issueRmCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "rm <KEY>",
		Short: "Delete an issue (cascades sub-issues)",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()
			k, err := domain.ParseIssueKey(args[0])
			if err != nil {
				return err
			}
			return s.DeleteIssue(k)
		},
	}
}
