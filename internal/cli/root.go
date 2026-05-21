package cli

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/alex/cliban/internal/store"
	"github.com/spf13/cobra"
)

type Globals struct {
	DBPath string
	JSON   bool
}

var G = &Globals{}

func NewRoot() *cobra.Command {
	root := &cobra.Command{
		Use:           "cliban",
		Short:         "AI-agent-first kanban board for the terminal",
		SilenceUsage:  true,
		SilenceErrors: true,
	}
	root.PersistentFlags().StringVar(&G.DBPath, "db", "", "path to SQLite DB (default: $CLIBAN_DB or $XDG_DATA_HOME/cliban/cliban.db)")
	root.AddCommand(NewInit(), NewVersion(), newProjectCmd(), newMilestoneCmd(), newIssueCmd(), newLabelCmd(), newTUICmd(), newFFFCmd())
	root.RunE = func(cmd *cobra.Command, args []string) error {
		return RunTUI()
	}
	return root
}

func NewVersion() *cobra.Command {
	return &cobra.Command{
		Use:   "version",
		Short: "Print version",
		Run: func(cmd *cobra.Command, args []string) {
			fmt.Println("cliban dev")
		},
	}
}

func newTUICmd() *cobra.Command {
	return &cobra.Command{
		Use:   "tui",
		Short: "Open the kanban TUI",
		RunE: func(cmd *cobra.Command, args []string) error {
			return RunTUI()
		},
	}
}

func DefaultDBPath() (string, error) {
	if G.DBPath != "" {
		return G.DBPath, nil
	}
	if v := os.Getenv("CLIBAN_DB"); v != "" {
		return v, nil
	}
	var base string
	if v := os.Getenv("XDG_DATA_HOME"); v != "" {
		base = v
	} else {
		home, err := os.UserHomeDir()
		if err != nil {
			return "", fmt.Errorf("user home: %w", err)
		}
		base = filepath.Join(home, ".local", "share")
	}
	return filepath.Join(base, "cliban", "cliban.db"), nil
}

func openStore() (*store.Store, error) {
	path, err := DefaultDBPath()
	if err != nil {
		return nil, err
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return nil, fmt.Errorf("create db dir: %w", err)
	}
	s, err := store.Open(path)
	if err != nil {
		return nil, err
	}
	if err := s.Migrate(); err != nil {
		_ = s.Close()
		return nil, err
	}
	return s, nil
}

var RunTUI = func() error {
	fmt.Println("TUI not yet implemented")
	return nil
}
