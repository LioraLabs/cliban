package cli

import (
	"bytes"
	"encoding/json"
	"strings"
	"testing"

	"github.com/alex/cliban/internal/domain"
)

func TestJSONIssue(t *testing.T) {
	buf := &bytes.Buffer{}
	i := &domain.Issue{Seq: 42, Title: "x", Status: domain.StatusBacklog, Priority: domain.PriorityHigh}
	if err := WriteIssueJSON(buf, "CLI", i); err != nil {
		t.Fatal(err)
	}
	var got map[string]any
	if err := json.Unmarshal(buf.Bytes(), &got); err != nil {
		t.Fatalf("decode: %v\n%s", err, buf.String())
	}
	if got["key"] != "CLI-42" || got["title"] != "x" || got["status"] != "backlog" {
		t.Errorf("unexpected fields: %v", got)
	}
}

func TestTableIssues(t *testing.T) {
	buf := &bytes.Buffer{}
	items := []ListIssueRow{
		{Key: "CLI-1", Title: "first", Status: "backlog", Priority: "high"},
		{Key: "CLI-2", Title: "second", Status: "done", Priority: "low"},
	}
	WriteIssueTable(buf, items)
	out := buf.String()
	if !strings.Contains(out, "CLI-1") || !strings.Contains(out, "CLI-2") {
		t.Errorf("missing rows: %s", out)
	}
	if !strings.Contains(out, "first") || !strings.Contains(out, "second") {
		t.Errorf("missing titles: %s", out)
	}
}
