package cli

import (
	"fmt"
	"os"
	"strings"

	"github.com/alex/cliban/internal/search"
	"github.com/alex/cliban/internal/store"
	"github.com/alex/cliban/internal/tui"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/spf13/cobra"
	"golang.org/x/term"
)

// newFFFCmd builds the `cliban fff` cobra subcommand. The command surfaces a
// fuzzy issue picker; when stdin is not a TTY it falls into "batch" mode and
// emits NDJSON matches to stdout, which is what `runCLI` (and pipelines) use.
// The interactive picker itself lands in Task 10 — `runFFFPicker` is stubbed.
func newFFFCmd() *cobra.Command {
	var (
		project, label, milestone, status, priority, parent string
		showFlag, editFlag, jsonFlag                        bool
		archived, noSubs                                    bool
	)
	c := &cobra.Command{
		Use:   "fff [QUERY]",
		Short: "Fuzzy-find issues; print selected key to stdout",
		Args:  cobra.MaximumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			// At most one of --show, --edit, --json may be set.
			modes := 0
			for _, b := range []bool{showFlag, editFlag, jsonFlag} {
				if b {
					modes++
				}
			}
			if modes > 1 {
				return fmt.Errorf("%w: --show, --edit, and --json are mutually exclusive", store.ErrValidation)
			}

			query := ""
			if len(args) == 1 {
				query = args[0]
			}

			stdinTTY := term.IsTerminal(int(os.Stdin.Fd()))

			if !stdinTTY {
				// Batch mode (tests, pipes). Query is required because there is
				// no interactive way to refine the result set.
				if strings.TrimSpace(query) == "" {
					return fmt.Errorf("%w: cliban fff in non-interactive mode requires a QUERY", store.ErrValidation)
				}
				return runFFFBatch(cmd, query, project, label, milestone, status, priority, parent, archived, noSubs)
			}

			// stdin is a TTY. Whether stdout is a TTY (interactive) or a pipe
			// (e.g. `cliban fff | xargs ...`), the picker still runs; the
			// selected key just lands wherever stdout points.
			return runFFFPicker(cmd, query, project, label, milestone, status, priority, parent, archived, noSubs, showFlag, editFlag, jsonFlag)
		},
	}
	c.Flags().StringVar(&project, "project", "", "project key filter")
	c.Flags().StringVar(&label, "label", "", "label name filter")
	c.Flags().StringVar(&milestone, "milestone", "", "milestone filter")
	c.Flags().StringVar(&status, "status", "", "status filter")
	c.Flags().StringVar(&priority, "priority", "", "priority filter")
	c.Flags().StringVar(&parent, "parent", "", "list sub-issues of this parent key")
	c.Flags().BoolVar(&archived, "archived", false, "include archived issues")
	c.Flags().BoolVar(&noSubs, "no-subs", false, "exclude sub-issues")
	c.Flags().BoolVar(&showFlag, "show", false, "after picking, open `issue show` for the selection (v1.1 — currently stubbed)")
	c.Flags().BoolVar(&editFlag, "edit", false, "after picking, open `issue edit --editor` for the selection (v1.1 — currently stubbed)")
	c.Flags().BoolVar(&jsonFlag, "json", false, "emit full issue JSON instead of just the key (v1.1 — currently stubbed)")
	_ = c.Flags().MarkHidden("show")
	_ = c.Flags().MarkHidden("edit")
	_ = c.Flags().MarkHidden("json")
	return c
}

// runFFFBatch runs the search in non-interactive mode and writes one NDJSON
// search-match line per result to the command's stdout. Filter handling
// mirrors `runIssueSearch` so the two surfaces stay consistent.
func runFFFBatch(cmd *cobra.Command, query, project, label, milestone, status, priority, parent string, archived, noSubs bool) error {
	s, err := openStore()
	if err != nil {
		return err
	}
	defer s.Close()
	opts := search.Options{
		Query:           query,
		Projects:        singletonOrNil(strings.ToUpper(project)),
		Labels:          singletonOrNil(label),
		Milestones:      singletonOrNil(milestone),
		Statuses:        singletonOrNil(status),
		Priorities:      singletonOrNil(priority),
		IncludeArchived: archived,
		ExcludeSubs:     noSubs,
		ParentKey:       parent,
		Limit:           50,
	}
	matches, err := search.Search(cmd.Context(), s, opts)
	if err != nil {
		return err
	}
	projects := projectKeysByID(s)
	out := cmd.OutOrStdout()
	for _, m := range matches {
		pk := projects[m.Issue.ProjectID]
		if err := WriteSearchMatchNDJSON(out, issueJSONInputs(s, projects, pk, m.Issue), m.Score); err != nil {
			return err
		}
	}
	return nil
}

// runFFFPicker drives the interactive Bubble Tea picker. We take a single
// snapshot of the candidate set via search.Search at picker open (so we
// don't hit SQLite on every keystroke), feed it into tui.PickerModel which
// re-ranks in-memory with sahilm/fuzzy, and on Enter print the selected key
// to stdout. The picker UI is drawn to stderr so `key=$(cliban fff)` works
// — classic fzf pattern: UI on tty/stderr, result on stdout.
func runFFFPicker(cmd *cobra.Command, query, project, label, milestone, status, priority, parent string, archived, noSubs, showFlag, editFlag, jsonFlag bool) error {
	s, err := openStore()
	if err != nil {
		return err
	}
	defer s.Close()

	// One-shot fetch at picker open. We pass an empty Query so the search
	// surface returns the full candidate set; the picker filters in memory
	// from there. If the caller passed an initial QUERY we'll seed it into
	// the textinput below, which triggers an immediate in-memory rerank
	// without a second store round-trip.
	opts := search.Options{
		Query:           "",
		Projects:        singletonOrNil(strings.ToUpper(project)),
		Labels:          singletonOrNil(label),
		Milestones:      singletonOrNil(milestone),
		Statuses:        singletonOrNil(status),
		Priorities:      singletonOrNil(priority),
		IncludeArchived: archived,
		ExcludeSubs:     noSubs,
		ParentKey:       parent,
	}
	matches, err := search.Search(cmd.Context(), s, opts)
	if err != nil {
		return err
	}

	items := make([]tui.PickerItem, len(matches))
	for i, m := range matches {
		items[i] = tui.PickerItem{
			Key:      fmt.Sprintf("%s-%d", m.ProjectKey, m.Issue.Seq),
			Title:    m.Issue.Title,
			Project:  m.ProjectKey,
			Status:   string(m.Issue.Status),
			Priority: string(m.Issue.Priority),
			Labels:   m.Labels,
		}
	}

	model := tui.NewPickerModel(items)
	if q := strings.TrimSpace(query); q != "" {
		model = model.WithInitialQuery(q)
	}

	// Draw UI to stderr so stdout stays clean for the selected key. This
	// also means `cliban fff | xargs ...` works end-to-end with the picker
	// still rendering on the user's tty.
	p := tea.NewProgram(model, tea.WithOutput(os.Stderr))
	finalModel, err := p.Run()
	if err != nil {
		return err
	}
	fm := finalModel.(tui.PickerModel)
	if fm.Cancelled() {
		// Esc / Ctrl-C with no selection → exit 1 (spec C.1; Ctrl-C → 130 is
		// the nice-to-have that bubbletea doesn't differentiate from Esc at
		// this layer in v1).
		os.Exit(1)
	}
	sel := fm.Selected()
	if sel == nil {
		os.Exit(1)
	}

	// Output modes. --show/--edit/--json post-selection invocations are
	// stubbed for v1: CLI users can compose with $(cliban fff) until the
	// follow-up wires the inner show/edit/json paths in-process.
	switch {
	case showFlag:
		return fmt.Errorf("--show not yet wired in v1; use `cliban issue show %s`", sel.Key)
	case editFlag:
		return fmt.Errorf("--edit not yet wired in v1; use `cliban issue edit %s --editor`", sel.Key)
	case jsonFlag:
		return fmt.Errorf("--json output for selected item not yet wired in v1; use `cliban issue show %s --json`", sel.Key)
	default:
		fmt.Fprintln(cmd.OutOrStdout(), sel.Key)
		return nil
	}
}
