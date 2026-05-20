package cli

import (
	"fmt"
	"io"
	"os"
	"os/exec"
	"regexp"
	"strings"

	"github.com/alex/cliban/internal/domain"
	"github.com/alex/cliban/internal/store"
	"github.com/spf13/cobra"
)

func issueTickCmd() *cobra.Command {
	var taskN, stepM int
	var asJSON bool
	c := &cobra.Command{
		Use:   "tick <KEY>",
		Short: "Atomically tick a step in the issue's ## Plan section",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			k, err := domain.ParseIssueKey(args[0])
			if err != nil {
				return err
			}
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()
			res, err := s.TickStep(k, taskN, stepM)
			if err != nil {
				return err
			}
			if asJSON {
				return WriteJSON(cmd.OutOrStdout(), map[string]any{
					"key":        args[0],
					"task":       res.TaskN,
					"step":       res.StepM,
					"checked":    res.Checked,
					"updated_at": res.UpdatedAt,
				})
			}
			fmt.Fprintf(cmd.OutOrStdout(), "ticked %s Task %d Step %d\n", args[0], res.TaskN, res.StepM)
			return nil
		},
	}
	c.Flags().IntVar(&taskN, "task", 0, "task number (required, 1-indexed)")
	c.Flags().IntVar(&stepM, "step", 0, "step number (required, 1-indexed)")
	c.Flags().BoolVar(&asJSON, "json", false, "JSON output")
	_ = c.MarkFlagRequired("task")
	_ = c.MarkFlagRequired("step")
	return c
}

// branchIssueRE matches a cliban-style git branch name and captures the
// project key + seq. Example: "cli-12-fix-column-ordering" → ("cli", "12").
var branchIssueRE = regexp.MustCompile(`^([a-z][a-z0-9]+)-(\d+)(?:-|$)`)

// currentBranch returns the current git branch name. The
// CLIBAN_CURRENT_BRANCH_OVERRIDE env var lets tests substitute a value
// without invoking git.
func currentBranch() (string, error) {
	if v := os.Getenv("CLIBAN_CURRENT_BRANCH_OVERRIDE"); v != "" {
		return v, nil
	}
	cmd := exec.Command("git", "branch", "--show-current")
	out, err := cmd.Output()
	if err != nil {
		return "", fmt.Errorf("git branch --show-current: %w", err)
	}
	return strings.TrimSpace(string(out)), nil
}

func issueCurrentCmd() *cobra.Command {
	var asJSON bool
	c := &cobra.Command{
		Use:   "current",
		Short: "Show the issue inferred from the current git branch",
		RunE: func(cmd *cobra.Command, args []string) error {
			branch, err := currentBranch()
			if err != nil {
				return err
			}
			match := branchIssueRE.FindStringSubmatch(branch)
			if match == nil {
				return fmt.Errorf("%w: no issue found for current branch %q", store.ErrNotFound, branch)
			}
			key := domain.IssueKey{Project: strings.ToUpper(match[1])}
			fmt.Sscanf(match[2], "%d", &key.Seq)
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()
			issue, err := s.GetIssueByKey(key)
			if err != nil {
				return fmt.Errorf("%w: no issue found for current branch %q (parsed %s)", store.ErrNotFound, branch, key)
			}
			projects := projectKeysByID(s)
			pk := projects[issue.ProjectID]
			if asJSON {
				return WriteIssueJSON(cmd.OutOrStdout(), issueJSONInputs(s, projects, pk, issue))
			}
			fmt.Fprintf(cmd.OutOrStdout(), "%s-%d %s\n", pk, issue.Seq, issue.Title)
			return nil
		},
	}
	c.Flags().BoolVar(&asJSON, "json", false, "JSON output")
	return c
}

func issueLogCmd() *cobra.Command {
	var messageFile string
	var asJSON bool
	c := &cobra.Command{
		Use:   "log <KEY> [<message>]",
		Short: "Atomically append an entry to the issue's ## Activity Log section",
		Args:  cobra.RangeArgs(1, 2),
		RunE: func(cmd *cobra.Command, args []string) error {
			k, err := domain.ParseIssueKey(args[0])
			if err != nil {
				return err
			}
			msg := ""
			if len(args) == 2 {
				msg = args[1]
			}
			if cmd.Flags().Changed("message-file") {
				if msg != "" {
					return fmt.Errorf("%w: pass <message> OR --message-file, not both", store.ErrValidation)
				}
				if messageFile == "-" {
					b, err := io.ReadAll(cmd.InOrStdin())
					if err != nil {
						return err
					}
					msg = strings.TrimRight(string(b), "\n")
				} else {
					b, err := os.ReadFile(messageFile)
					if err != nil {
						return fmt.Errorf("%w: %v", store.ErrValidation, err)
					}
					msg = strings.TrimRight(string(b), "\n")
				}
			}
			if msg == "" {
				return fmt.Errorf("%w: message required (positional or --message-file)", store.ErrValidation)
			}
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()
			res, err := s.AppendActivityLog(k, msg)
			if err != nil {
				return err
			}
			if asJSON {
				return WriteJSON(cmd.OutOrStdout(), map[string]any{
					"key":       args[0],
					"entry":     res.Entry,
					"timestamp": res.Timestamp,
				})
			}
			fmt.Fprintf(cmd.OutOrStdout(), "logged on %s: %s\n", args[0], res.Entry)
			return nil
		},
	}
	c.Flags().StringVar(&messageFile, "message-file", "", "read message from file (use '-' for stdin)")
	c.Flags().BoolVar(&asJSON, "json", false, "JSON output")
	return c
}

func issuePromoteCmd() *cobra.Command {
	var taskN, stepM int
	var title, asMode string
	var asJSON bool
	c := &cobra.Command{
		Use:   "promote <KEY>",
		Short: "Promote a plan step into its own issue and rewrite the step line",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			k, err := domain.ParseIssueKey(args[0])
			if err != nil {
				return err
			}
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()
			res, err := s.PromoteStep(store.PromoteParams{
				Parent: k,
				TaskN:  taskN,
				StepM:  stepM,
				Title:  title,
				Mode:   store.PromoteMode(asMode),
			})
			if err != nil {
				return err
			}
			if asJSON {
				return WriteJSON(cmd.OutOrStdout(), map[string]any{
					"parent":  args[0],
					"task":    res.TaskN,
					"step":    res.StepM,
					"new_key": res.NewKey.String(),
				})
			}
			fmt.Fprintf(cmd.OutOrStdout(), "promoted %s Task %d Step %d → %s\n", args[0], res.TaskN, res.StepM, res.NewKey)
			return nil
		},
	}
	c.Flags().IntVar(&taskN, "task", 0, "task number (required, 1-indexed)")
	c.Flags().IntVar(&stepM, "step", 0, "step number (required, 1-indexed)")
	c.Flags().StringVar(&title, "title", "", "title for the promoted issue (required)")
	c.Flags().StringVar(&asMode, "as", "sub-issue", "promotion mode: sub-issue|related")
	c.Flags().BoolVar(&asJSON, "json", false, "JSON output")
	_ = c.MarkFlagRequired("task")
	_ = c.MarkFlagRequired("step")
	_ = c.MarkFlagRequired("title")
	return c
}
