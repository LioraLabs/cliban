package cli

import (
	"fmt"
	"strings"
	"time"

	"github.com/alex/cliban/internal/domain"
	"github.com/alex/cliban/internal/store"
	"github.com/spf13/cobra"
)

func milestoneCmd() *cobra.Command {
	c := &cobra.Command{Use: "milestone", Short: "Manage milestones"}
	c.AddCommand(milestoneAddCmd(), milestoneListCmd(), milestoneShowCmd(), milestoneEditCmd(), milestoneRmCmd())
	return c
}

func parseTarget(s string) (*time.Time, error) {
	if s == "" {
		return nil, nil
	}
	t, err := time.Parse("2006-01-02", s)
	if err != nil {
		return nil, fmt.Errorf("invalid --target %q (want YYYY-MM-DD)", s)
	}
	return &t, nil
}

func milestoneAddCmd() *cobra.Command {
	var project, name, desc, target string
	var asJSON bool
	c := &cobra.Command{
		Use:   "add",
		Short: "Add a milestone",
		RunE: func(cmd *cobra.Command, args []string) error {
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()
			tgt, err := parseTarget(target)
			if err != nil {
				return err
			}
			m, err := s.CreateMilestone(strings.ToUpper(project), name, desc, tgt)
			if err != nil {
				return err
			}
			if asJSON {
				return WriteJSON(cmd.OutOrStdout(), milestoneToJSON(s, m))
			}
			fmt.Fprintf(cmd.OutOrStdout(), "created milestone %s in %s\n", m.Name, strings.ToUpper(project))
			return nil
		},
	}
	c.Flags().StringVar(&project, "project", "", "project key (required)")
	c.Flags().StringVar(&name, "name", "", "milestone name (required)")
	c.Flags().StringVar(&desc, "description", "", "description")
	c.Flags().StringVar(&target, "target", "", "target date YYYY-MM-DD")
	c.Flags().BoolVar(&asJSON, "json", false, "JSON output")
	_ = c.MarkFlagRequired("project")
	_ = c.MarkFlagRequired("name")
	return c
}

func milestoneListCmd() *cobra.Command {
	var project, status string
	var asJSON bool
	c := &cobra.Command{
		Use:   "ls",
		Short: "List milestones",
		RunE: func(cmd *cobra.Command, args []string) error {
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()
			ms, err := s.ListMilestones(strings.ToUpper(project), status)
			if err != nil {
				return err
			}
			if asJSON {
				for _, m := range ms {
					if err := WriteJSONLine(cmd.OutOrStdout(), milestoneToJSON(s, m)); err != nil {
						return err
					}
				}
				return nil
			}
			for _, m := range ms {
				tgt := "-"
				if m.TargetDate != nil {
					tgt = m.TargetDate.Format("2006-01-02")
				}
				fmt.Fprintf(cmd.OutOrStdout(), "%-15s %-10s %s\n", m.Name, m.Status, tgt)
			}
			return nil
		},
	}
	c.Flags().StringVar(&project, "project", "", "project key (required)")
	c.Flags().StringVar(&status, "status", "", "filter by status")
	c.Flags().BoolVar(&asJSON, "json", false, "NDJSON output")
	_ = c.MarkFlagRequired("project")
	return c
}

func milestoneShowCmd() *cobra.Command {
	var project, nameFlag string
	var asJSON, withIssues bool
	c := &cobra.Command{
		Use:   "show [NAME]",
		Short: "Show a milestone (accepts positional NAME or --name)",
		Args:  cobra.MaximumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()
			name := nameFlag
			if len(args) == 1 {
				if name != "" && name != args[0] {
					return fmt.Errorf("%w: pass NAME positionally OR via --name, not both", store.ErrValidation)
				}
				name = args[0]
			}
			if name == "" {
				return fmt.Errorf("%w: milestone name required (positional or --name)", store.ErrValidation)
			}
			if project == "" {
				return fmt.Errorf("%w: --project is required", store.ErrValidation)
			}
			projectKey := strings.ToUpper(project)
			m, err := s.GetMilestone(projectKey, name)
			if err != nil {
				return err
			}
			// Resolve issue list for milestone (uses listing in the project).
			issues, _ := s.ListIssues(store.ListIssuesFilter{ProjectKey: projectKey, MilestoneName: name})
			projects := projectKeysByID(s)
			if asJSON {
				obj := milestoneToJSON(s, m)
				obj["issue_count"] = len(issues)
				if withIssues {
					list := make([]map[string]any, 0, len(issues))
					for _, i := range issues {
						list = append(list, IssueToJSON(issueJSONInputs(s, projects, projectKey, i)))
					}
					obj["issues"] = list
				}
				return WriteJSON(cmd.OutOrStdout(), obj)
			}
			tgt := "-"
			if m.TargetDate != nil {
				tgt = m.TargetDate.Format("2006-01-02")
			}
			fmt.Fprintf(cmd.OutOrStdout(), "%s — %s\nstatus:  %s\ntarget:  %s\nissues:  %d\n%s\n",
				m.Name, projectKey, m.Status, tgt, len(issues), m.Description)
			if withIssues {
				fmt.Fprintln(cmd.OutOrStdout())
				rows := []ListIssueRow{}
				for _, i := range issues {
					pk := projects[i.ProjectID]
					parentKey, msName := resolveIssueRefs(s, projects, i)
					rows = append(rows, ListIssueRow{
						Key:       fmt.Sprintf("%s-%d", pk, i.Seq),
						Title:     i.Title,
						Status:    string(i.Status),
						Priority:  string(i.Priority),
						Milestone: msName,
						Parent:    parentKey,
					})
				}
				WriteIssueTable(cmd.OutOrStdout(), rows)
			}
			return nil
		},
	}
	c.Flags().StringVar(&project, "project", "", "project key (required)")
	c.Flags().StringVar(&nameFlag, "name", "", "milestone name (or pass positionally)")
	c.Flags().BoolVar(&asJSON, "json", false, "JSON output")
	c.Flags().BoolVar(&withIssues, "with-issues", false, "include the milestone's issues in the output")
	_ = c.MarkFlagRequired("project")
	return c
}

func milestoneEditCmd() *cobra.Command {
	var project, name, rename, desc, status, target string
	var clearTarget bool
	c := &cobra.Command{
		Use:   "edit",
		Short: "Edit a milestone",
		RunE: func(cmd *cobra.Command, args []string) error {
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()
			params := store.UpdateMilestoneParams{}
			if cmd.Flags().Changed("rename") {
				params.NewName = &rename
			}
			if cmd.Flags().Changed("description") {
				params.Description = &desc
			}
			if cmd.Flags().Changed("status") {
				params.Status = &status
			}
			if clearTarget {
				params.ClearTarget = true
			} else if cmd.Flags().Changed("target") {
				tgt, err := parseTarget(target)
				if err != nil {
					return err
				}
				params.TargetDate = tgt
			}
			return s.UpdateMilestone(strings.ToUpper(project), name, params)
		},
	}
	c.Flags().StringVar(&project, "project", "", "project key (required)")
	c.Flags().StringVar(&name, "name", "", "milestone name (required)")
	c.Flags().StringVar(&rename, "rename", "", "new name")
	c.Flags().StringVar(&desc, "description", "", "new description")
	c.Flags().StringVar(&status, "status", "", "new status (open|completed|cancelled)")
	c.Flags().StringVar(&target, "target", "", "new target date YYYY-MM-DD")
	c.Flags().BoolVar(&clearTarget, "clear-target", false, "clear target date")
	_ = c.MarkFlagRequired("project")
	_ = c.MarkFlagRequired("name")
	return c
}

func milestoneRmCmd() *cobra.Command {
	var project, name string
	c := &cobra.Command{
		Use:   "rm",
		Short: "Delete a milestone",
		RunE: func(cmd *cobra.Command, args []string) error {
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()
			return s.DeleteMilestone(strings.ToUpper(project), name)
		},
	}
	c.Flags().StringVar(&project, "project", "", "project key (required)")
	c.Flags().StringVar(&name, "name", "", "milestone name (required)")
	_ = c.MarkFlagRequired("project")
	_ = c.MarkFlagRequired("name")
	return c
}

func milestoneToJSON(s *store.Store, m *domain.Milestone) map[string]any {
	out := map[string]any{
		"name":        m.Name,
		"description": m.Description,
		"status":      string(m.Status),
		"created_at":  m.CreatedAt,
		"updated_at":  m.UpdatedAt,
	}
	if m.TargetDate != nil {
		out["target_date"] = m.TargetDate.Format("2006-01-02")
	} else {
		out["target_date"] = nil
	}
	if s != nil {
		issues, err := s.ListIssues(store.ListIssuesFilter{MilestoneName: m.Name})
		if err == nil {
			// Filter to the same project as this milestone.
			projects := projectKeysByID(s)
			pk := projects[m.ProjectID]
			out["project"] = pk
			count := 0
			for _, i := range issues {
				if i.ProjectID == m.ProjectID {
					count++
				}
			}
			out["issue_count"] = count
		}
	}
	return out
}
