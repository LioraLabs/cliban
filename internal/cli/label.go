package cli

import (
	"fmt"
	"strings"

	"github.com/spf13/cobra"
)

func labelCmd() *cobra.Command {
	c := &cobra.Command{Use: "label", Short: "Manage labels"}
	c.AddCommand(labelAddCmd(), labelListCmd(), labelRmCmd())
	return c
}

func labelAddCmd() *cobra.Command {
	var project string
	c := &cobra.Command{
		Use:   "add <NAME>",
		Short: "Create a label in a project",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()
			return s.CreateLabel(strings.ToUpper(project), args[0])
		},
	}
	c.Flags().StringVar(&project, "project", "", "project key (required)")
	_ = c.MarkFlagRequired("project")
	return c
}

func labelListCmd() *cobra.Command {
	var project string
	var asJSON bool
	c := &cobra.Command{
		Use:   "ls",
		Short: "List labels in a project",
		RunE: func(cmd *cobra.Command, args []string) error {
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()
			labels, err := s.ListLabels(strings.ToUpper(project))
			if err != nil {
				return err
			}
			if asJSON {
				for _, n := range labels {
					if err := WriteJSONLine(cmd.OutOrStdout(), map[string]any{"name": n}); err != nil {
						return err
					}
				}
				return nil
			}
			for _, n := range labels {
				fmt.Fprintln(cmd.OutOrStdout(), n)
			}
			return nil
		},
	}
	c.Flags().StringVar(&project, "project", "", "project key (required)")
	c.Flags().BoolVar(&asJSON, "json", false, "NDJSON output")
	_ = c.MarkFlagRequired("project")
	return c
}

func labelRmCmd() *cobra.Command {
	var project string
	c := &cobra.Command{
		Use:   "rm <NAME>",
		Short: "Delete a label (also detaches it from all issues)",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()
			return s.DeleteLabel(strings.ToUpper(project), args[0])
		},
	}
	c.Flags().StringVar(&project, "project", "", "project key (required)")
	_ = c.MarkFlagRequired("project")
	return c
}
