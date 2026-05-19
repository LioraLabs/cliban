package cli

import (
	"strings"
	"testing"

	"github.com/alex/cliban/internal/domain"
)

func TestSerializeBuffer(t *testing.T) {
	bf := IssueBuffer{
		Header:      "# Editing CLI-1",
		Title:       "Hello",
		Status:      "backlog",
		Priority:    "high",
		Milestone:   "v0.1",
		Parent:      "CLI-12",
		Description: "Some markdown body.\n\nWith multiple paragraphs.",
	}
	out := bf.Serialize()
	if !strings.Contains(out, "title:     Hello") {
		t.Errorf("title formatting: %s", out)
	}
	if !strings.HasPrefix(out, "# Editing CLI-1") {
		t.Errorf("header not first: %s", out)
	}
	if strings.Count(out, "\n---\n") != 2 {
		t.Errorf("want two '---' lines, got: %s", out)
	}
}

func TestParseBufferRoundtrip(t *testing.T) {
	src := "# Editing CLI-1 — lines above the first '---' are ignored.\n" +
		"---\n" +
		"title:     Hello\n" +
		"status:    in-progress\n" +
		"priority:  medium\n" +
		"milestone: v0.1\n" +
		"parent:    CLI-12\n" +
		"---\n" +
		"Body text with **markdown**.\n\n" +
		"# Heading inside body is fine.\n"
	got, err := ParseIssueBuffer(src)
	if err != nil {
		t.Fatalf("ParseIssueBuffer: %v", err)
	}
	if got.Title != "Hello" || got.Status != "in-progress" || got.Priority != "medium" {
		t.Errorf("front matter: %+v", got)
	}
	if got.Milestone != "v0.1" || got.Parent != "CLI-12" {
		t.Errorf("links: %+v", got)
	}
	if !strings.Contains(got.Description, "**markdown**") {
		t.Errorf("body lost: %q", got.Description)
	}
	if !strings.Contains(got.Description, "# Heading inside body is fine.") {
		t.Errorf("body heading stripped: %q", got.Description)
	}
}

func TestParseBufferEmpty(t *testing.T) {
	if _, err := ParseIssueBuffer(""); err == nil {
		t.Error("want error on empty buffer")
	}
}

func TestParseBufferInvalidStatus(t *testing.T) {
	src := "---\ntitle: x\nstatus: nope\npriority: low\nmilestone:\nparent:\n---\nbody\n"
	if _, err := ParseIssueBuffer(src); err == nil {
		t.Error("want error on invalid status")
	}
}

func TestParseBufferAllowsEmptyClearValues(t *testing.T) {
	src := "---\ntitle: x\nstatus: backlog\npriority: none\nmilestone: ''\nparent: ''\n---\n\n"
	b, err := ParseIssueBuffer(src)
	if err != nil {
		t.Fatal(err)
	}
	if b.Milestone != "" || b.Parent != "" {
		t.Errorf("empties not cleared: %+v", b)
	}
}

var _ domain.Status = domain.StatusBacklog
