package cli

import (
	"encoding/json"
	"fmt"
	"io"
	"text/tabwriter"

	"github.com/alex/cliban/internal/domain"
)

type ListIssueRow struct {
	Key      string `json:"key"`
	Title    string `json:"title"`
	Status   string `json:"status"`
	Priority string `json:"priority"`
	Parent   string `json:"parent,omitempty"`
}

func IssueToJSON(projectKey string, i *domain.Issue, parent string, milestone string) map[string]any {
	out := map[string]any{
		"key":         fmt.Sprintf("%s-%d", projectKey, i.Seq),
		"title":       i.Title,
		"description": i.Description,
		"status":      string(i.Status),
		"priority":    string(i.Priority),
		"position":    i.Position,
		"created_at":  i.CreatedAt,
		"updated_at":  i.UpdatedAt,
	}
	if parent != "" {
		out["parent"] = parent
	}
	if milestone != "" {
		out["milestone"] = milestone
	}
	if i.CompletedAt != nil {
		out["completed_at"] = *i.CompletedAt
	}
	return out
}

func WriteIssueJSON(w io.Writer, projectKey string, i *domain.Issue) error {
	enc := json.NewEncoder(w)
	enc.SetIndent("", "  ")
	return enc.Encode(IssueToJSON(projectKey, i, "", ""))
}

func WriteIssueTable(w io.Writer, rows []ListIssueRow) {
	tw := tabwriter.NewWriter(w, 0, 0, 2, ' ', 0)
	fmt.Fprintln(tw, "KEY\tTITLE\tSTATUS\tPRIORITY")
	for _, r := range rows {
		fmt.Fprintf(tw, "%s\t%s\t%s\t%s\n", r.Key, r.Title, r.Status, r.Priority)
	}
	tw.Flush()
}

func WriteJSON(w io.Writer, v any) error {
	enc := json.NewEncoder(w)
	enc.SetIndent("", "  ")
	return enc.Encode(v)
}

func WriteNDJSON(w io.Writer, items []any) error {
	enc := json.NewEncoder(w)
	for _, v := range items {
		if err := enc.Encode(v); err != nil {
			return err
		}
	}
	return nil
}
