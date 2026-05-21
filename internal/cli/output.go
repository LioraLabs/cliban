package cli

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"regexp"
	"strings"
	"text/tabwriter"

	"github.com/alex/cliban/internal/domain"
)

type ListIssueRow struct {
	Key       string `json:"key"`
	Title     string `json:"title"`
	Status    string `json:"status"`
	Priority  string `json:"priority"`
	Milestone string `json:"milestone,omitempty"`
	Parent    string `json:"parent,omitempty"`
}

// IssueRelationOut describes one outgoing relation on an issue.
type IssueRelationOut struct {
	Type   string `json:"type"`   // "blocks" | "blocked_by" | "related_to"
	Target string `json:"target"` // e.g. "CLI-2"
}

// IssueJSONInputs collects the resolved data needed to render an issue to JSON.
// Optional fields use empty/nil to mean "absent" — the renderer emits null/[]
// so the schema is stable for downstream consumers.
type IssueJSONInputs struct {
	ProjectKey string
	Issue      *domain.Issue
	Parent     string             // resolved parent issue key, e.g. "CLI-1"
	Milestone  string             // resolved milestone name
	Due        string             // YYYY-MM-DD
	Labels     []string           // label names
	Relations  []IssueRelationOut // outgoing relations
}

// IssueToJSON builds the canonical JSON object for an issue. milestone,
// parent, due_date, labels, relations, and git_branch_name are always
// present (null / empty arrays when absent) so the schema is stable.
func IssueToJSON(in IssueJSONInputs) map[string]any {
	i := in.Issue
	key := fmt.Sprintf("%s-%d", in.ProjectKey, i.Seq)
	labels := in.Labels
	if labels == nil {
		labels = []string{}
	}
	relations := in.Relations
	if relations == nil {
		relations = []IssueRelationOut{}
	}
	out := map[string]any{
		"key":             key,
		"title":           i.Title,
		"description":     i.Description,
		"status":          string(i.Status),
		"priority":        string(i.Priority),
		"position":        i.Position,
		"archived":        i.Archived,
		"milestone":       nilIfEmpty(in.Milestone),
		"parent":          nilIfEmpty(in.Parent),
		"due_date":        nilIfEmpty(in.Due),
		"labels":          labels,
		"relations":       relations,
		"git_branch_name": gitBranchName(key, i.Title),
		"created_at":      i.CreatedAt,
		"updated_at":      i.UpdatedAt,
	}
	if i.CompletedAt != nil {
		out["completed_at"] = *i.CompletedAt
	}
	return out
}

func nilIfEmpty(s string) any {
	if s == "" {
		return nil
	}
	return s
}

// WriteIssueJSON writes a pretty-printed JSON object for a single issue.
func WriteIssueJSON(w io.Writer, in IssueJSONInputs) error {
	return writeJSONPretty(w, IssueToJSON(in))
}

// WriteIssueNDJSON writes a single compact JSON line for an issue (list use).
func WriteIssueNDJSON(w io.Writer, in IssueJSONInputs) error {
	return writeJSONLine(w, IssueToJSON(in))
}

// WriteSearchMatchNDJSON writes a single NDJSON line for a search match,
// adding a "score" field to the standard issue JSON shape.
func WriteSearchMatchNDJSON(w io.Writer, in IssueJSONInputs, score int) error {
	out := IssueToJSON(in)
	out["score"] = score
	return writeJSONLine(w, out)
}

func WriteIssueTable(w io.Writer, rows []ListIssueRow) {
	tw := tabwriter.NewWriter(w, 0, 0, 2, ' ', 0)
	fmt.Fprintln(tw, "KEY\tTITLE\tSTATUS\tPRIORITY\tMILESTONE\tPARENT")
	for _, r := range rows {
		fmt.Fprintf(tw, "%s\t%s\t%s\t%s\t%s\t%s\n",
			r.Key, r.Title, r.Status, r.Priority, dashIfEmpty(r.Milestone), dashIfEmpty(r.Parent))
	}
	tw.Flush()
}

// ListSearchRow mirrors ListIssueRow with an extra Score column for fuzzy
// search output.
type ListSearchRow struct {
	Score     int    `json:"score"`
	Key       string `json:"key"`
	Title     string `json:"title"`
	Status    string `json:"status"`
	Priority  string `json:"priority"`
	Milestone string `json:"milestone,omitempty"`
	Parent    string `json:"parent,omitempty"`
}

// WriteSearchTable renders fuzzy search rows as a human-readable table with a
// leading SCORE column.
func WriteSearchTable(w io.Writer, rows []ListSearchRow) {
	tw := tabwriter.NewWriter(w, 0, 0, 2, ' ', 0)
	fmt.Fprintln(tw, "SCORE\tKEY\tTITLE\tSTATUS\tPRIORITY\tMILESTONE\tPARENT")
	for _, r := range rows {
		fmt.Fprintf(tw, "%d\t%s\t%s\t%s\t%s\t%s\t%s\n",
			r.Score, r.Key, r.Title, r.Status, r.Priority, dashIfEmpty(r.Milestone), dashIfEmpty(r.Parent))
	}
	tw.Flush()
}

func dashIfEmpty(s string) string {
	if s == "" {
		return "-"
	}
	return s
}

// WriteJSON writes a pretty-printed JSON object.
func WriteJSON(w io.Writer, v any) error {
	return writeJSONPretty(w, v)
}

// WriteJSONLine writes a single compact JSON line (NDJSON-style).
func WriteJSONLine(w io.Writer, v any) error {
	return writeJSONLine(w, v)
}

func writeJSONPretty(w io.Writer, v any) error {
	enc := json.NewEncoder(w)
	enc.SetIndent("", "  ")
	return enc.Encode(v)
}

func writeJSONLine(w io.Writer, v any) error {
	var buf bytes.Buffer
	enc := json.NewEncoder(&buf)
	enc.SetEscapeHTML(false)
	if err := enc.Encode(v); err != nil {
		return err
	}
	_, err := w.Write(buf.Bytes())
	return err
}

var nonSlugRE = regexp.MustCompile(`[^a-z0-9]+`)

// gitBranchName builds a git-friendly branch name from a key and title.
// Format: <key-lower>-<slugified-title>, e.g. "cli-12-fix-column-ordering".
func gitBranchName(key, title string) string {
	keyLower := strings.ToLower(key)
	slug := nonSlugRE.ReplaceAllString(strings.ToLower(title), "-")
	slug = strings.Trim(slug, "-")
	if slug == "" {
		return keyLower
	}
	if len(slug) > 60 {
		slug = strings.TrimRight(slug[:60], "-")
	}
	return keyLower + "-" + slug
}
