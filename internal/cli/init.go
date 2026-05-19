package cli

import (
	"fmt"

	"github.com/spf13/cobra"
)

func NewInit() *cobra.Command {
	return &cobra.Command{
		Use:   "init",
		Short: "Initialize the cliban SQLite database",
		RunE: func(cmd *cobra.Command, args []string) error {
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()
			path, _ := DefaultDBPath()
			fmt.Fprintf(cmd.OutOrStdout(), "initialized cliban db at %s\n", path)
			return nil
		},
	}
}

// Placeholders so root compiles; real impls land in Tasks 10-12.
func newProjectCmd() *cobra.Command { return projectCmd() }
func newMilestoneCmd() *cobra.Command { return &cobra.Command{Use: "milestone", Short: "(stub)"} }
func newIssueCmd() *cobra.Command     { return &cobra.Command{Use: "issue", Short: "(stub)"} }
