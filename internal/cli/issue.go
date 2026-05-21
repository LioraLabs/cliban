package cli

import (
	"bufio"
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"sort"
	"strings"
	"time"

	"github.com/alex/cliban/internal/descmd"
	"github.com/alex/cliban/internal/domain"
	"github.com/alex/cliban/internal/search"
	"github.com/alex/cliban/internal/store"
	"github.com/spf13/cobra"
)

func issueCmd() *cobra.Command {
	c := &cobra.Command{Use: "issue", Short: "Manage issues"}
	c.AddCommand(issueAddCmd(), issueListCmd(), issueShowCmd(), issueEditCmd(), issueMvCmd(), issueRmCmd(),
		issueArchiveCmd(), issueUnarchiveCmd(), issueArchiveDoneCmd(), issueImportCmd(), issueBlockedCmd(),
		issueCurrentCmd(), issueTickCmd(), issueLogCmd(), issuePromoteCmd())
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

// resolveDescription returns the effective description string considering
// --description, --description-file and stdin ('-').
func resolveDescription(desc, descFile string, descChanged, descFileChanged bool) (string, bool, error) {
	if descFileChanged {
		if descChanged {
			return "", false, fmt.Errorf("%w: --description and --description-file are mutually exclusive", store.ErrValidation)
		}
		if descFile == "-" {
			data, err := io.ReadAll(os.Stdin)
			if err != nil {
				return "", false, err
			}
			return string(data), true, nil
		}
		data, err := os.ReadFile(descFile)
		if err != nil {
			return "", false, fmt.Errorf("%w: %v", store.ErrValidation, err)
		}
		return string(data), true, nil
	}
	if descChanged {
		v, err := readMaybeStdin(desc)
		if err != nil {
			return "", false, err
		}
		return v, true, nil
	}
	return "", false, nil
}

// resolveIssueRefs returns the parent issue key (e.g. "CLI-2") and milestone
// name for an issue, looking them up in the store. Returns empty strings if
// the corresponding field is not set or cannot be resolved.
func resolveIssueRefs(s *store.Store, projects map[int64]string, i *domain.Issue) (parent, milestone string) {
	if i.ParentID != nil {
		if p, err := s.GetIssueByID(*i.ParentID); err == nil && p != nil {
			if pk, ok := projects[p.ProjectID]; ok {
				parent = fmt.Sprintf("%s-%d", pk, p.Seq)
			}
		}
	}
	if i.MilestoneID != nil {
		if m, err := s.GetMilestoneByID(*i.MilestoneID); err == nil && m != nil {
			milestone = m.Name
		}
	}
	return
}

// issueJSONInputs builds an IssueJSONInputs for a given issue, resolving
// milestone name, parent key, labels, relations, and the due date.
func issueJSONInputs(s *store.Store, projects map[int64]string, projectKey string, i *domain.Issue) IssueJSONInputs {
	parent, milestone := resolveIssueRefs(s, projects, i)
	in := IssueJSONInputs{
		ProjectKey: projectKey,
		Issue:      i,
		Parent:     parent,
		Milestone:  milestone,
	}
	if i.DueDate != nil {
		in.Due = i.DueDate.UTC().Format("2006-01-02")
	}
	if labels, err := s.LabelsForIssue(i.ID); err == nil {
		in.Labels = labels
	}
	if rels, err := s.RelationsForIssue(i.ID); err == nil {
		out := make([]IssueRelationOut, 0, len(rels))
		for _, r := range rels {
			out = append(out, IssueRelationOut{Type: string(r.Kind), Target: fmt.Sprintf("%s-%d", r.Target.Project, r.Target.Seq)})
		}
		in.Relations = out
	}
	return in
}

func issueAddCmd() *cobra.Command {
	var project, title, desc, descFile, parent, milestone, priority, status, due string
	var labels, blocks, blockedBy, relatedTo []string
	var asJSON, noEditor, editorFlag bool
	c := &cobra.Command{
		Use:   "add",
		Short: "Add an issue (pass --editor to open $EDITOR for input)",
		RunE: func(cmd *cobra.Command, args []string) error {
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()

			descChanged := cmd.Flags().Changed("description")
			descFileChanged := cmd.Flags().Changed("description-file")

			descContent, descSet, err := resolveDescription(desc, descFile, descChanged, descFileChanged)
			if err != nil {
				return err
			}
			contentless := title == "" && !descSet
			editorRequested := editorFlag && !EditorDisabled(noEditor)
			// Editor path: only when explicitly requested and no title/desc supplied.
			if contentless && editorRequested {
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
				return printIssueResult(cmd.OutOrStdout(), s, params.ProjectKey, issue, "created", asJSON)
			}
			if contentless {
				return fmt.Errorf("%w: --title required (pass --editor to open $EDITOR)", store.ErrValidation)
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
			if due != "" {
				t, err := parseDueDate(due)
				if err != nil {
					return err
				}
				params.DueDate = t
			}
			issue, err := s.CreateIssue(params)
			if err != nil {
				return err
			}
			issueKey := domain.IssueKey{Project: params.ProjectKey, Seq: issue.Seq}
			for _, lbl := range labels {
				if err := s.AttachLabel(issueKey, lbl); err != nil {
					return err
				}
			}
			if err := applyRelationFlags(s, issueKey, blocks, blockedBy, relatedTo); err != nil {
				return err
			}
			// Reload to pick up relations/labels in JSON output.
			fresh, _ := s.GetIssueByKey(issueKey)
			if fresh != nil {
				issue = fresh
			}
			return printIssueResult(cmd.OutOrStdout(), s, params.ProjectKey, issue, "created", asJSON)
		},
	}
	c.Flags().StringVar(&project, "project", "", "project key (required)")
	c.Flags().StringVar(&title, "title", "", "issue title")
	c.Flags().StringVar(&desc, "description", "", "description (use '-' to read from stdin)")
	c.Flags().StringVar(&descFile, "description-file", "", "read description from a file (use '-' for stdin)")
	c.Flags().StringVar(&parent, "parent", "", "parent issue key (sub-issue)")
	c.Flags().StringVar(&milestone, "milestone", "", "milestone name")
	c.Flags().StringVar(&priority, "priority", "", "priority")
	c.Flags().StringVar(&status, "status", "", "status")
	c.Flags().StringVar(&due, "due", "", "due date YYYY-MM-DD")
	c.Flags().StringSliceVar(&labels, "label", nil, "label name (repeatable)")
	c.Flags().StringSliceVar(&blocks, "blocks", nil, "this issue blocks KEY (repeatable)")
	c.Flags().StringSliceVar(&blockedBy, "blocked-by", nil, "this issue is blocked by KEY (repeatable)")
	c.Flags().StringSliceVar(&relatedTo, "related-to", nil, "this issue relates to KEY (repeatable)")
	c.Flags().BoolVar(&asJSON, "json", false, "JSON output")
	c.Flags().BoolVar(&editorFlag, "editor", false, "open $EDITOR for input when no --title supplied")
	c.Flags().BoolVar(&noEditor, "no-editor", false, "deprecated: editor is opt-in via --editor; kept for backwards compatibility")
	_ = c.Flags().MarkHidden("no-editor")
	_ = c.MarkFlagRequired("project")
	return c
}

// parseDueDate parses YYYY-MM-DD into a UTC time pointer.
func parseDueDate(s string) (*time.Time, error) {
	t, err := time.Parse("2006-01-02", s)
	if err != nil {
		return nil, fmt.Errorf("%w: invalid --due %q (want YYYY-MM-DD)", store.ErrValidation, s)
	}
	return &t, nil
}

// applyRelationFlags applies --blocks/--blocked-by/--related-to flags to an issue.
func applyRelationFlags(s *store.Store, k domain.IssueKey, blocks, blockedBy, relatedTo []string) error {
	for _, raw := range blocks {
		other, err := domain.ParseIssueKey(raw)
		if err != nil {
			return err
		}
		if err := s.AddRelation(k, other, store.RelBlocks); err != nil {
			return err
		}
	}
	for _, raw := range blockedBy {
		other, err := domain.ParseIssueKey(raw)
		if err != nil {
			return err
		}
		// blocked-by = other blocks this
		if err := s.AddRelation(other, k, store.RelBlocks); err != nil {
			return err
		}
	}
	for _, raw := range relatedTo {
		other, err := domain.ParseIssueKey(raw)
		if err != nil {
			return err
		}
		if err := s.AddRelation(k, other, store.RelRelatedTo); err != nil {
			return err
		}
	}
	return nil
}

func printIssueResult(w io.Writer, s *store.Store, projectKey string, issue *domain.Issue, verb string, asJSON bool) error {
	if asJSON {
		projects := projectKeysByID(s)
		return WriteIssueJSON(w, issueJSONInputs(s, projects, projectKey, issue))
	}
	fmt.Fprintf(w, "%s %s-%d: %s\n", verb, projectKey, issue.Seq, issue.Title)
	return nil
}

func issueListCmd() *cobra.Command {
	var project, status, priority, milestone, parent, sortBy string
	var updatedSinceFlag, searchQuery string
	var labels []string
	var noSubs, asJSON, includeArchived bool
	var limit int
	c := &cobra.Command{
		Use:   "ls",
		Short: "List issues",
		RunE: func(cmd *cobra.Command, args []string) error {
			if cmd.Flags().Changed("search") && strings.TrimSpace(searchQuery) == "" {
				return fmt.Errorf("%w: --search requires a non-empty query", store.ErrValidation)
			}
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()
			if strings.TrimSpace(searchQuery) != "" {
				if cmd.Flags().Changed("sort") {
					fmt.Fprintf(cmd.ErrOrStderr(), "note: --sort is ignored when --search is set\n")
				}
				return runIssueSearch(cmd, s, issueSearchInputs{
					query:           searchQuery,
					project:         project,
					status:          status,
					priority:        priority,
					milestone:       milestone,
					parent:          parent,
					labels:          labels,
					includeArchived: includeArchived,
					noSubs:          noSubs,
					limit:           limit,
					asJSON:          asJSON,
				})
			}
			f := store.ListIssuesFilter{
				ProjectKey:      strings.ToUpper(project),
				MilestoneName:   milestone,
				LabelNames:      labels,
				NoSubs:          noSubs,
				IncludeArchived: includeArchived,
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
			if updatedSinceFlag != "" {
				ts, err := parseUpdatedSince(updatedSinceFlag, time.Now())
				if err != nil {
					return err
				}
				f.UpdatedSince = &ts
			}
			issues, err := s.ListIssues(f)
			if err != nil {
				return err
			}
			if sortBy != "" {
				if err := sortIssues(issues, sortBy); err != nil {
					return err
				}
			}
			projects := projectKeysByID(s)
			if asJSON {
				for _, i := range issues {
					pk := projects[i.ProjectID]
					if err := WriteIssueNDJSON(cmd.OutOrStdout(), issueJSONInputs(s, projects, pk, i)); err != nil {
						return err
					}
				}
				return nil
			}
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
			return nil
		},
	}
	c.Flags().StringVar(&project, "project", "", "project key filter")
	c.Flags().StringVar(&status, "status", "", "status filter")
	c.Flags().StringVar(&priority, "priority", "", "priority filter")
	c.Flags().StringVar(&milestone, "milestone", "", "milestone filter")
	c.Flags().StringVar(&parent, "parent", "", "list sub-issues of this parent key")
	c.Flags().StringVar(&sortBy, "sort", "", "sort key: priority|created|updated|position[:asc|desc]")
	c.Flags().StringSliceVar(&labels, "label", nil, "filter to issues with ALL of these labels (repeatable)")
	c.Flags().BoolVar(&noSubs, "no-subs", false, "exclude sub-issues")
	c.Flags().BoolVar(&asJSON, "json", false, "NDJSON output (one compact JSON object per line)")
	c.Flags().BoolVar(&includeArchived, "archived", false, "include archived issues")
	c.Flags().StringVar(&updatedSinceFlag, "updated-since", "", "filter issues updated within a duration (e.g. 4h) or since an RFC3339 timestamp")
	c.Flags().StringVar(&searchQuery, "search", "", "fuzzy search query across title/key/labels/description")
	c.Flags().IntVar(&limit, "limit", 0, "cap result count (default 50 when --search is set; ignored otherwise)")
	return c
}

// issueSearchInputs bundles the resolved CLI flags for the --search branch
// of `issue ls` so the helper signature stays compact.
type issueSearchInputs struct {
	query           string
	project         string
	status          string
	priority        string
	milestone       string
	parent          string
	labels          []string
	includeArchived bool
	noSubs          bool
	limit           int
	asJSON          bool
}

// runIssueSearch performs a fuzzy search using internal/search and writes the
// results either as NDJSON (with a `score` field per row) or as a tabular
// listing with a leading SCORE column.
func runIssueSearch(cmd *cobra.Command, s *store.Store, in issueSearchInputs) error {
	effectiveLimit := in.limit
	if effectiveLimit == 0 {
		effectiveLimit = 50
	}
	opts := search.Options{
		Query:           in.query,
		Projects:        singletonOrNil(strings.ToUpper(in.project)),
		Labels:          in.labels,
		Milestones:      singletonOrNil(in.milestone),
		Statuses:        singletonOrNil(in.status),
		Priorities:      singletonOrNil(in.priority),
		IncludeArchived: in.includeArchived,
		ExcludeSubs:     in.noSubs,
		ParentKey:       in.parent,
		Limit:           effectiveLimit,
	}
	matches, err := search.Search(cmd.Context(), s, opts)
	if err != nil {
		return err
	}
	projects := projectKeysByID(s)
	out := cmd.OutOrStdout()
	if in.asJSON {
		for _, m := range matches {
			pk := projects[m.Issue.ProjectID]
			if err := WriteSearchMatchNDJSON(out, issueJSONInputs(s, projects, pk, m.Issue), m.Score); err != nil {
				return err
			}
		}
		return nil
	}
	rows := make([]ListSearchRow, 0, len(matches))
	for _, m := range matches {
		pk := projects[m.Issue.ProjectID]
		parentKey, msName := resolveIssueRefs(s, projects, m.Issue)
		rows = append(rows, ListSearchRow{
			Score:     m.Score,
			Key:       fmt.Sprintf("%s-%d", pk, m.Issue.Seq),
			Title:     m.Issue.Title,
			Status:    string(m.Issue.Status),
			Priority:  string(m.Issue.Priority),
			Milestone: msName,
			Parent:    parentKey,
		})
	}
	WriteSearchTable(out, rows)
	return nil
}

// singletonOrNil returns nil for an empty string, else a one-element slice.
// Used to translate CLI string flags into the slice-shaped filter fields of
// search.Options.
func singletonOrNil(s string) []string {
	if s == "" {
		return nil
	}
	return []string{s}
}

// sortIssues sorts issues in place by the given key. Accepts "<field>" or
// "<field>:asc"/"<field>:desc". Default direction is asc, except for priority
// which defaults to desc (urgent first).
func sortIssues(issues []*domain.Issue, spec string) error {
	field := spec
	dir := ""
	if idx := strings.Index(spec, ":"); idx >= 0 {
		field = spec[:idx]
		dir = spec[idx+1:]
	}
	desc := false
	switch dir {
	case "", "asc":
		desc = false
	case "desc":
		desc = true
	default:
		return fmt.Errorf("%w: invalid sort direction %q (use asc or desc)", store.ErrValidation, dir)
	}
	switch field {
	case "priority":
		if dir == "" {
			desc = true
		}
		less := func(a, b *domain.Issue) bool {
			return domain.PriorityRank(a.Priority) < domain.PriorityRank(b.Priority)
		}
		sort.SliceStable(issues, func(a, b int) bool {
			if desc {
				return less(issues[b], issues[a])
			}
			return less(issues[a], issues[b])
		})
	case "created":
		sort.SliceStable(issues, func(a, b int) bool {
			if desc {
				return issues[a].CreatedAt.After(issues[b].CreatedAt)
			}
			return issues[a].CreatedAt.Before(issues[b].CreatedAt)
		})
	case "updated":
		sort.SliceStable(issues, func(a, b int) bool {
			if desc {
				return issues[a].UpdatedAt.After(issues[b].UpdatedAt)
			}
			return issues[a].UpdatedAt.Before(issues[b].UpdatedAt)
		})
	case "position":
		sort.SliceStable(issues, func(a, b int) bool {
			if desc {
				return issues[a].Position > issues[b].Position
			}
			return issues[a].Position < issues[b].Position
		})
	default:
		return fmt.Errorf("%w: invalid --sort field %q (priority|created|updated|position)", store.ErrValidation, field)
	}
	return nil
}

// parseUpdatedSince accepts either a duration ("4h", "30m") or an
// RFC3339 timestamp and returns the absolute UTC time to filter from.
func parseUpdatedSince(s string, now time.Time) (time.Time, error) {
	if d, err := time.ParseDuration(s); err == nil {
		return now.UTC().Add(-d), nil
	}
	if t, err := time.Parse(time.RFC3339, s); err == nil {
		return t.UTC(), nil
	}
	if t, err := time.Parse(time.RFC3339Nano, s); err == nil {
		return t.UTC(), nil
	}
	return time.Time{}, fmt.Errorf("%w: invalid --updated-since %q (want duration like 4h or RFC3339 timestamp)", store.ErrValidation, s)
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

// sectionAnchor maps a --section short name to the canonical H2 anchor text.
func sectionAnchor(s string) (string, error) {
	switch s {
	case "spec":
		return "Spec", nil
	case "plan":
		return "Plan", nil
	case "activity":
		return "Activity Log", nil
	case "notes":
		return "Notes", nil
	default:
		return "", fmt.Errorf("%w: invalid --section %q (want spec|plan|activity|notes)", store.ErrValidation, s)
	}
}

func issueShowCmd() *cobra.Command {
	var asJSON, usePager bool
	var section string
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
			// --section is mutually exclusive with --json and --pager; it's a
			// targeted machine read.
			if section != "" {
				anchor, err := sectionAnchor(section)
				if err != nil {
					return err
				}
				start, end, ok := descmd.FindSection(issue.Description, anchor)
				if !ok {
					return fmt.Errorf("%w: no ## %s section in %s", store.ErrNotFound, anchor, args[0])
				}
				fmt.Fprint(cmd.OutOrStdout(), issue.Description[start:end])
				return nil
			}
			projects := projectKeysByID(s)
			if asJSON {
				return WriteIssueJSON(cmd.OutOrStdout(), issueJSONInputs(s, projects, k.Project, issue))
			}
			parentKey, msName := resolveIssueRefs(s, projects, issue)
			body := fmt.Sprintf("%s — %s\nstatus:    %s\npriority:  %s\nmilestone: %s\nparent:    %s\n\n%s\n",
				k, issue.Title, issue.Status, issue.Priority,
				dashIfEmpty(msName), dashIfEmpty(parentKey), issue.Description)
			if usePager {
				return runPager(cmd.OutOrStdout(), []byte(body))
			}
			fmt.Fprint(cmd.OutOrStdout(), body)
			return nil
		},
	}
	c.Flags().BoolVar(&asJSON, "json", false, "JSON output")
	c.Flags().StringVar(&section, "section", "", "show only one section: spec|plan|activity|notes")
	c.Flags().BoolVar(&usePager, "pager", false, "pipe human-readable output through $PAGER")
	return c
}

// runPager pipes the given bytes through $PAGER. If $PAGER is unset, falls
// back to writing directly to fallback (the command's stdout).
func runPager(fallback io.Writer, content []byte) error {
	pager := os.Getenv("PAGER")
	if pager == "" {
		_, err := fallback.Write(content)
		return err
	}
	cmd := exec.Command("sh", "-c", pager)
	cmd.Stdin = bytes.NewReader(content)
	cmd.Stdout = fallback
	cmd.Stderr = os.Stderr
	return cmd.Run()
}

func issueEditCmd() *cobra.Command {
	var title, desc, descFile, priority, milestone, parent, due string
	var addLabels, removeLabels, blocks, blockedBy, relatedTo, removeRelations []string
	var clearMilestone, clearParent, clearDue, editorFlag, noEditor, asJSON bool
	c := &cobra.Command{
		Use:   "edit <KEY>",
		Short: "Edit an issue (pass --editor to open $EDITOR)",
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
			descChanged := cmd.Flags().Changed("description")
			descFileChanged := cmd.Flags().Changed("description-file")
			if descChanged || descFileChanged {
				v, set, err := resolveDescription(desc, descFile, descChanged, descFileChanged)
				if err != nil {
					return err
				}
				if set {
					params.Description = &v
				}
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
			if clearDue {
				params.ClearDueDate = true
			} else if cmd.Flags().Changed("due") {
				t, err := parseDueDate(due)
				if err != nil {
					return err
				}
				params.DueDate = t
			}
			anyChange := cmd.Flags().Changed("title") || descChanged || descFileChanged ||
				cmd.Flags().Changed("priority") || cmd.Flags().Changed("milestone") ||
				cmd.Flags().Changed("parent") || clearMilestone || clearParent ||
				cmd.Flags().Changed("due") || clearDue ||
				len(addLabels) > 0 || len(removeLabels) > 0 ||
				len(blocks) > 0 || len(blockedBy) > 0 || len(relatedTo) > 0 || len(removeRelations) > 0
			editorRequested := editorFlag && !EditorDisabled(noEditor)
			if !anyChange && editorRequested {
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
					if m, err := s.GetMilestoneByID(*cur.MilestoneID); err == nil && m != nil {
						bf.Milestone = m.Name
					}
				}
				if cur.ParentID != nil {
					if p, err := s.GetIssueByID(*cur.ParentID); err == nil && p != nil {
						bf.Parent = fmt.Sprintf("%s-%d", k.Project, p.Seq)
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
				if err := s.UpdateIssue(k, up); err != nil {
					return err
				}
				updated, err := s.GetIssueByKey(k)
				if err != nil {
					return err
				}
				return printIssueResult(cmd.OutOrStdout(), s, k.Project, updated, "updated", asJSON)
			}
			if !anyChange {
				return fmt.Errorf("%w: no edits requested (pass a flag or --editor)", store.ErrValidation)
			}
			if err := s.UpdateIssue(k, params); err != nil {
				return err
			}
			for _, lbl := range addLabels {
				if err := s.AttachLabel(k, lbl); err != nil {
					return err
				}
			}
			for _, lbl := range removeLabels {
				if err := s.DetachLabel(k, lbl); err != nil {
					return err
				}
			}
			if err := applyRelationFlags(s, k, blocks, blockedBy, relatedTo); err != nil {
				return err
			}
			for _, raw := range removeRelations {
				other, err := domain.ParseIssueKey(raw)
				if err != nil {
					return err
				}
				_ = s.RemoveRelation(k, other, store.RelBlocks)
				_ = s.RemoveRelation(other, k, store.RelBlocks)
				_ = s.RemoveRelation(k, other, store.RelRelatedTo)
			}
			updated, err := s.GetIssueByKey(k)
			if err != nil {
				return err
			}
			return printIssueResult(cmd.OutOrStdout(), s, k.Project, updated, "updated", asJSON)
		},
	}
	c.Flags().StringVar(&title, "title", "", "new title")
	c.Flags().StringVar(&desc, "description", "", "new description (use '-' for stdin)")
	c.Flags().StringVar(&descFile, "description-file", "", "read description from a file (use '-' for stdin)")
	c.Flags().StringVar(&priority, "priority", "", "new priority")
	c.Flags().StringVar(&milestone, "milestone", "", "new milestone")
	c.Flags().BoolVar(&clearMilestone, "clear-milestone", false, "clear milestone")
	c.Flags().StringVar(&parent, "parent", "", "new parent key")
	c.Flags().BoolVar(&clearParent, "clear-parent", false, "clear parent")
	c.Flags().StringVar(&due, "due", "", "new due date YYYY-MM-DD")
	c.Flags().BoolVar(&clearDue, "clear-due", false, "clear due date")
	c.Flags().StringSliceVar(&addLabels, "label", nil, "add label (repeatable)")
	c.Flags().StringSliceVar(&removeLabels, "remove-label", nil, "remove label (repeatable)")
	c.Flags().StringSliceVar(&blocks, "blocks", nil, "add 'blocks' relation to KEY (repeatable)")
	c.Flags().StringSliceVar(&blockedBy, "blocked-by", nil, "add 'blocked by' relation from KEY (repeatable)")
	c.Flags().StringSliceVar(&relatedTo, "related-to", nil, "add 'related to' relation to KEY (repeatable)")
	c.Flags().StringSliceVar(&removeRelations, "remove-relation", nil, "remove any relation involving KEY (repeatable)")
	c.Flags().BoolVarP(&editorFlag, "editor", "e", false, "open $EDITOR for full edit")
	c.Flags().BoolVar(&noEditor, "no-editor", false, "deprecated: editor is opt-in via --editor")
	_ = c.Flags().MarkHidden("no-editor")
	c.Flags().BoolVar(&asJSON, "json", false, "JSON output")
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

func issueArchiveCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "archive <KEY>",
		Short: "Archive an issue (hides it from the default board and lists)",
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
			return s.SetIssueArchived(k, true)
		},
	}
}

func issueUnarchiveCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "unarchive <KEY>",
		Short: "Unarchive an issue",
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
			return s.SetIssueArchived(k, false)
		},
	}
}

func issueArchiveDoneCmd() *cobra.Command {
	var project string
	var asJSON, auto bool
	c := &cobra.Command{
		Use:   "archive-done",
		Short: "Archive done issues. By default archives every done issue in --project; --auto honors each project's auto_archive_done_after_days policy.",
		RunE: func(cmd *cobra.Command, args []string) error {
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()
			if auto {
				n, err := s.SweepAutoArchive()
				if err != nil {
					return err
				}
				if asJSON {
					return WriteJSON(cmd.OutOrStdout(), map[string]any{"archived": n, "mode": "auto"})
				}
				fmt.Fprintf(cmd.OutOrStdout(), "archived %d done issue(s) (auto sweep)\n", n)
				return nil
			}
			if project == "" {
				return fmt.Errorf("%w: --project is required (or use --auto for the per-project policy)", store.ErrValidation)
			}
			n, err := s.ArchiveDoneInProject(strings.ToUpper(project))
			if err != nil {
				return err
			}
			if asJSON {
				return WriteJSON(cmd.OutOrStdout(), map[string]any{"archived": n})
			}
			fmt.Fprintf(cmd.OutOrStdout(), "archived %d done issue(s) in %s\n", n, strings.ToUpper(project))
			return nil
		},
	}
	c.Flags().StringVar(&project, "project", "", "project key")
	c.Flags().BoolVar(&auto, "auto", false, "sweep every project per its auto_archive_done_after_days policy")
	c.Flags().BoolVar(&asJSON, "json", false, "JSON output")
	return c
}

// IssueImportSpec is the per-line schema accepted by `cliban issue import`.
type IssueImportSpec struct {
	Project     string   `json:"project"`
	Title       string   `json:"title"`
	Description string   `json:"description,omitempty"`
	Status      string   `json:"status,omitempty"`
	Priority    string   `json:"priority,omitempty"`
	Milestone   string   `json:"milestone,omitempty"`
	Parent      string   `json:"parent,omitempty"`
	Labels      []string `json:"labels,omitempty"`
}

func issueImportCmd() *cobra.Command {
	var file, defaultProject string
	var asJSON bool
	c := &cobra.Command{
		Use:   "import [file]",
		Short: "Bulk-create issues from an NDJSON file (or stdin with '-')",
		Args:  cobra.MaximumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			path := file
			if len(args) == 1 {
				path = args[0]
			}
			var src io.Reader
			if path == "" || path == "-" {
				src = os.Stdin
			} else {
				f, err := os.Open(path)
				if err != nil {
					return err
				}
				defer f.Close()
				src = f
			}
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()
			projects := projectKeysByID(s)
			scanner := bufio.NewScanner(src)
			scanner.Buffer(make([]byte, 0, 64*1024), 4*1024*1024)
			lineNo := 0
			created := 0
			out := cmd.OutOrStdout()
			for scanner.Scan() {
				lineNo++
				line := strings.TrimSpace(scanner.Text())
				if line == "" || strings.HasPrefix(line, "#") {
					continue
				}
				var spec IssueImportSpec
				if err := json.Unmarshal([]byte(line), &spec); err != nil {
					return fmt.Errorf("line %d: invalid JSON: %w", lineNo, err)
				}
				if spec.Project == "" {
					spec.Project = defaultProject
				}
				if spec.Project == "" {
					return fmt.Errorf("%w: line %d: project required (set per-record or pass --project)", store.ErrValidation, lineNo)
				}
				params := store.CreateIssueParams{
					ProjectKey:    strings.ToUpper(spec.Project),
					Title:         spec.Title,
					Description:   spec.Description,
					MilestoneName: spec.Milestone,
				}
				if spec.Status != "" {
					st, err := domain.ParseStatus(spec.Status)
					if err != nil {
						return fmt.Errorf("line %d: %w", lineNo, err)
					}
					params.Status = st
				}
				if spec.Priority != "" {
					pr, err := domain.ParsePriority(spec.Priority)
					if err != nil {
						return fmt.Errorf("line %d: %w", lineNo, err)
					}
					params.Priority = pr
				}
				if spec.Parent != "" {
					pk, err := domain.ParseIssueKey(spec.Parent)
					if err != nil {
						return fmt.Errorf("line %d: %w", lineNo, err)
					}
					params.ParentKey = &pk
				}
				issue, err := s.CreateIssue(params)
				if err != nil {
					return fmt.Errorf("line %d: %w", lineNo, err)
				}
				for _, lbl := range spec.Labels {
					if err := s.AttachLabel(domain.IssueKey{Project: params.ProjectKey, Seq: issue.Seq}, lbl); err != nil {
						return fmt.Errorf("line %d: attach label %q: %w", lineNo, lbl, err)
					}
				}
				created++
				if asJSON {
					if err := WriteIssueNDJSON(out, issueJSONInputs(s, projects, params.ProjectKey, issue)); err != nil {
						return err
					}
				}
			}
			if err := scanner.Err(); err != nil {
				return err
			}
			if !asJSON {
				fmt.Fprintf(out, "imported %d issue(s)\n", created)
			}
			return nil
		},
	}
	c.Flags().StringVar(&file, "file", "", "NDJSON file path (default: stdin)")
	c.Flags().StringVar(&defaultProject, "project", "", "default project key for records that omit it")
	c.Flags().BoolVar(&asJSON, "json", false, "emit each created issue as a JSON line")
	return c
}

func issueBlockedCmd() *cobra.Command {
	var project string
	var asJSON bool
	c := &cobra.Command{
		Use:   "blocked",
		Short: "List issues that have at least one open blocker",
		RunE: func(cmd *cobra.Command, args []string) error {
			s, err := openStore()
			if err != nil {
				return err
			}
			defer s.Close()
			projects := projectKeysByID(s)
			issues, err := s.ListBlockedIssues(strings.ToUpper(project))
			if err != nil {
				return err
			}
			if asJSON {
				for _, i := range issues {
					pk := projects[i.ProjectID]
					if err := WriteIssueNDJSON(cmd.OutOrStdout(), issueJSONInputs(s, projects, pk, i)); err != nil {
						return err
					}
				}
				return nil
			}
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
			return nil
		},
	}
	c.Flags().StringVar(&project, "project", "", "project key filter")
	c.Flags().BoolVar(&asJSON, "json", false, "NDJSON output")
	return c
}
