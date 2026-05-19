package cli

import (
	"fmt"
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
			m, err := s.CreateMilestone(project, name, desc, tgt)
			if err != nil {
				return err
			}
			if asJSON {
				return WriteJSON(cmd.OutOrStdout(), milestoneToJSON(m))
			}
			fmt.Fprintf(cmd.OutOrStdout(), "created milestone %s in %s\n", m.Name, project)
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
			ms, err := s.ListMilestones(project, status)
			if err != nil {
				return err
			}
			if asJSON {
				for _, m := range ms {
					if err := WriteJSON(cmd.OutOrStdout(), milestoneToJSON(m)); err != nil {
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
	var project, name string
	var asJSON bool
	c := &cobra.Command{
		Use:   "show",
		Short: "Show a milestone",
		RunE: func(cmd *cobra.Command, args []string) error {
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()
			m, err := s.GetMilestone(project, name)
			if err != nil {
				return err
			}
			if asJSON {
				return WriteJSON(cmd.OutOrStdout(), milestoneToJSON(m))
			}
			tgt := "-"
			if m.TargetDate != nil {
				tgt = m.TargetDate.Format("2006-01-02")
			}
			fmt.Fprintf(cmd.OutOrStdout(), "%s — %s\nstatus: %s\ntarget: %s\n%s\n", m.Name, project, m.Status, tgt, m.Description)
			return nil
		},
	}
	c.Flags().StringVar(&project, "project", "", "project key (required)")
	c.Flags().StringVar(&name, "name", "", "milestone name (required)")
	c.Flags().BoolVar(&asJSON, "json", false, "JSON output")
	_ = c.MarkFlagRequired("project")
	_ = c.MarkFlagRequired("name")
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
			return s.UpdateMilestone(project, name, params)
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
			return s.DeleteMilestone(project, name)
		},
	}
	c.Flags().StringVar(&project, "project", "", "project key (required)")
	c.Flags().StringVar(&name, "name", "", "milestone name (required)")
	_ = c.MarkFlagRequired("project")
	_ = c.MarkFlagRequired("name")
	return c
}

func milestoneToJSON(m *domain.Milestone) map[string]any {
	out := map[string]any{
		"name":        m.Name,
		"description": m.Description,
		"status":      string(m.Status),
		"created_at":  m.CreatedAt,
		"updated_at":  m.UpdatedAt,
	}
	if m.TargetDate != nil {
		out["target_date"] = m.TargetDate.Format("2006-01-02")
	}
	return out
}
