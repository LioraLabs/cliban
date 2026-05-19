package cli

import (
	"fmt"
	"strings"

	"github.com/alex/cliban/internal/domain"
	"github.com/alex/cliban/internal/store"
	"github.com/spf13/cobra"
)

func projectCmd() *cobra.Command {
	c := &cobra.Command{Use: "project", Short: "Manage projects"}
	c.AddCommand(projectAddCmd(), projectListCmd(), projectShowCmd(), projectEditCmd(),
		projectArchiveCmd(), projectUnarchiveCmd(), projectRmCmd())
	return c
}

func projectAddCmd() *cobra.Command {
	var name, desc string
	var asJSON bool
	c := &cobra.Command{
		Use:   "add <KEY>",
		Short: "Add a project (KEY must be uppercase letters/digits, 2-10 chars)",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()
			p, err := s.CreateProject(strings.ToUpper(args[0]), name, desc)
			if err != nil {
				return err
			}
			if asJSON {
				return WriteJSON(cmd.OutOrStdout(), projectToJSON(p))
			}
			fmt.Fprintf(cmd.OutOrStdout(), "created project %s (%s)\n", p.Key, p.Name)
			return nil
		},
	}
	c.Flags().StringVar(&name, "name", "", "human-readable name (required)")
	c.Flags().StringVar(&desc, "description", "", "description")
	c.Flags().BoolVar(&asJSON, "json", false, "print created project as JSON")
	_ = c.MarkFlagRequired("name")
	return c
}

func projectListCmd() *cobra.Command {
	var archived, asJSON bool
	c := &cobra.Command{
		Use:   "ls",
		Short: "List projects",
		RunE: func(cmd *cobra.Command, args []string) error {
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()
			ps, err := s.ListProjects(archived)
			if err != nil {
				return err
			}
			if asJSON {
				for _, p := range ps {
					if err := WriteJSON(cmd.OutOrStdout(), projectToJSON(p)); err != nil {
						return err
					}
				}
				return nil
			}
			for _, p := range ps {
				mark := ""
				if p.Archived {
					mark = " (archived)"
				}
				fmt.Fprintf(cmd.OutOrStdout(), "%-10s %s%s\n", p.Key, p.Name, mark)
			}
			return nil
		},
	}
	c.Flags().BoolVar(&archived, "archived", false, "include archived projects")
	c.Flags().BoolVar(&asJSON, "json", false, "NDJSON output")
	return c
}

func projectShowCmd() *cobra.Command {
	var asJSON bool
	c := &cobra.Command{
		Use:   "show <KEY>",
		Short: "Show a project",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()
			p, err := s.GetProjectByKey(strings.ToUpper(args[0]))
			if err != nil {
				return err
			}
			if asJSON {
				return WriteJSON(cmd.OutOrStdout(), projectToJSON(p))
			}
			fmt.Fprintf(cmd.OutOrStdout(), "%s — %s\n%s\n", p.Key, p.Name, p.Description)
			return nil
		},
	}
	c.Flags().BoolVar(&asJSON, "json", false, "JSON output")
	return c
}

func projectEditCmd() *cobra.Command {
	var name, desc string
	c := &cobra.Command{
		Use:   "edit <KEY>",
		Short: "Edit a project",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()
			key := strings.ToUpper(args[0])
			cur, err := s.GetProjectByKey(key)
			if err != nil {
				return err
			}
			if !cmd.Flags().Changed("name") {
				name = cur.Name
			}
			if !cmd.Flags().Changed("description") {
				desc = cur.Description
			}
			return s.UpdateProject(key, name, desc)
		},
	}
	c.Flags().StringVar(&name, "name", "", "new name")
	c.Flags().StringVar(&desc, "description", "", "new description")
	return c
}

func projectArchiveCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "archive <KEY>",
		Short: "Archive a project",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()
			return s.SetProjectArchived(strings.ToUpper(args[0]), true)
		},
	}
}

func projectUnarchiveCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "unarchive <KEY>",
		Short: "Unarchive a project",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()
			return s.SetProjectArchived(strings.ToUpper(args[0]), false)
		},
	}
}

func projectRmCmd() *cobra.Command {
	var force bool
	c := &cobra.Command{
		Use:   "rm <KEY>",
		Short: "Delete a project",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()
			key := strings.ToUpper(args[0])
			issues, err := s.ListIssues(store.ListIssuesFilter{ProjectKey: key})
			if err != nil {
				return err
			}
			if len(issues) > 0 && !force {
				return fmt.Errorf("%w: project %s has %d issues; pass --force to delete", store.ErrValidation, key, len(issues))
			}
			return s.DeleteProject(key)
		},
	}
	c.Flags().BoolVar(&force, "force", false, "delete even if project has issues")
	return c
}

func projectToJSON(p *domain.Project) map[string]any {
	return map[string]any{
		"key":         p.Key,
		"name":        p.Name,
		"description": p.Description,
		"archived":    p.Archived,
		"issue_seq":   p.IssueSeq,
		"created_at":  p.CreatedAt,
		"updated_at":  p.UpdatedAt,
	}
}
