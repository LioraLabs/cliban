package cli

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestIssueLsNDJSON verifies that --json on a list emits one compact JSON
// object per line and that milestone + parent are always present.
func TestIssueLsNDJSON(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "project", "add", "CLI", "--name", "Cliban"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "milestone", "add", "--project", "CLI", "--name", "v0.1"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "issue", "add", "--project", "CLI", "--title", "with milestone", "--milestone", "v0.1"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "issue", "add", "--project", "CLI", "--title", "without milestone"); c != 0 {
		t.Fatal()
	}
	out, _, c := runCLI(t, "issue", "ls", "--project", "CLI", "--json")
	if c != 0 {
		t.Fatalf("ls --json code=%d out=%s", c, out)
	}
	lines := strings.Split(strings.TrimRight(out, "\n"), "\n")
	if len(lines) != 2 {
		t.Fatalf("want 2 NDJSON lines, got %d: %q", len(lines), out)
	}
	withMs := false
	withoutMs := false
	for _, line := range lines {
		if strings.Contains(line, "\n") {
			t.Errorf("line contains internal newline (not compact): %q", line)
		}
		var obj map[string]any
		if err := json.Unmarshal([]byte(line), &obj); err != nil {
			t.Fatalf("not valid JSON: %v\n%s", err, line)
		}
		if _, ok := obj["milestone"]; !ok {
			t.Errorf("milestone key missing: %v", obj)
		}
		if _, ok := obj["parent"]; !ok {
			t.Errorf("parent key missing: %v", obj)
		}
		if obj["title"] == "with milestone" {
			if obj["milestone"] != "v0.1" {
				t.Errorf("expected milestone=v0.1, got %v", obj["milestone"])
			}
			withMs = true
		}
		if obj["title"] == "without milestone" {
			if obj["milestone"] != nil {
				t.Errorf("expected milestone=null, got %v", obj["milestone"])
			}
			withoutMs = true
		}
	}
	if !withMs || !withoutMs {
		t.Errorf("missing expected rows: withMs=%v withoutMs=%v", withMs, withoutMs)
	}
}

// TestIssueShowIncludesMilestoneAndParent verifies that show --json includes
// milestone and parent unconditionally.
func TestIssueShowIncludesMilestoneAndParent(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "project", "add", "CLI", "--name", "Cliban"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "issue", "add", "--project", "CLI", "--title", "parent"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "issue", "add", "--project", "CLI", "--title", "sub", "--parent", "CLI-1"); c != 0 {
		t.Fatal()
	}
	out, _, c := runCLI(t, "issue", "show", "CLI-2", "--json")
	if c != 0 {
		t.Fatalf("show code=%d out=%s", c, out)
	}
	var obj map[string]any
	if err := json.Unmarshal([]byte(out), &obj); err != nil {
		t.Fatalf("decode: %v\n%s", err, out)
	}
	if obj["parent"] != "CLI-1" {
		t.Errorf("expected parent=CLI-1, got %v", obj["parent"])
	}
	if _, ok := obj["milestone"]; !ok {
		t.Errorf("milestone key absent: %v", obj)
	}
	if obj["git_branch_name"] != "cli-2-sub" {
		t.Errorf("git_branch_name=%v want cli-2-sub", obj["git_branch_name"])
	}
}

// TestMilestoneShowPositional verifies the positional name form.
func TestMilestoneShowPositional(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "project", "add", "CLI", "--name", "Cliban"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "milestone", "add", "--project", "CLI", "--name", "v0.1"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "issue", "add", "--project", "CLI", "--title", "a", "--milestone", "v0.1"); c != 0 {
		t.Fatal()
	}
	out, _, c := runCLI(t, "milestone", "show", "v0.1", "--project", "CLI", "--with-issues", "--json")
	if c != 0 {
		t.Fatalf("show code=%d out=%s", c, out)
	}
	var obj map[string]any
	if err := json.Unmarshal([]byte(out), &obj); err != nil {
		t.Fatalf("decode: %v\n%s", err, out)
	}
	if obj["name"] != "v0.1" {
		t.Errorf("name=%v want v0.1", obj["name"])
	}
	if cnt, ok := obj["issue_count"].(float64); !ok || int(cnt) != 1 {
		t.Errorf("issue_count=%v want 1", obj["issue_count"])
	}
	issues, ok := obj["issues"].([]any)
	if !ok || len(issues) != 1 {
		t.Errorf("issues list missing: %v", obj["issues"])
	}
}

// TestIssueEditEchosUpdated verifies that edit emits a summary line on success.
func TestIssueEditEchosUpdated(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "project", "add", "CLI", "--name", "Cliban"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "issue", "add", "--project", "CLI", "--title", "x"); c != 0 {
		t.Fatal()
	}
	out, _, c := runCLI(t, "issue", "edit", "CLI-1", "--description", "hi")
	if c != 0 {
		t.Fatalf("edit code=%d out=%s", c, out)
	}
	if !strings.Contains(out, "updated CLI-1") {
		t.Errorf("expected 'updated CLI-1' line, got: %q", out)
	}
}

// TestIssueAddDefaultsToNoEditor verifies the inverted default.
func TestIssueAddDefaultsToNoEditor(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "project", "add", "CLI", "--name", "Cliban"); c != 0 {
		t.Fatal()
	}
	// No --title and no --editor → must fail with exit code 2 (validation).
	_, _, c := runCLI(t, "issue", "add", "--project", "CLI")
	if c != 2 {
		t.Errorf("expected exit 2 when no --title and no --editor, got %d", c)
	}
}

// TestIssueImportNDJSON exercises the bulk-import command.
func TestIssueImportNDJSON(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "project", "add", "CLI", "--name", "Cliban"); c != 0 {
		t.Fatal()
	}
	tmp := filepath.Join(t.TempDir(), "in.ndjson")
	contents := `{"project":"CLI","title":"alpha","priority":"high","labels":["bug"]}
{"project":"CLI","title":"beta","priority":"low"}
# comment lines are ignored

{"project":"CLI","title":"gamma"}
`
	if err := os.WriteFile(tmp, []byte(contents), 0o644); err != nil {
		t.Fatal(err)
	}
	out, _, c := runCLI(t, "issue", "import", tmp)
	if c != 0 {
		t.Fatalf("import code=%d out=%s", c, out)
	}
	if !strings.Contains(out, "imported 3 issue(s)") {
		t.Errorf("expected count line, got %q", out)
	}
	// Verify they were created and label attached.
	out, _, _ = runCLI(t, "issue", "ls", "--project", "CLI", "--label", "bug", "--json")
	if strings.Count(out, `"title":"alpha"`) != 1 {
		t.Errorf("label filter did not return alpha: %s", out)
	}
}

// TestIssueRelationsAndBlocked exercises --blocks/--blocked-by and the
// `issue blocked` query.
func TestIssueRelationsAndBlocked(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "project", "add", "CLI", "--name", "Cliban"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "issue", "add", "--project", "CLI", "--title", "blocker"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "issue", "add", "--project", "CLI", "--title", "stuck", "--blocked-by", "CLI-1"); c != 0 {
		t.Fatal()
	}
	out, _, c := runCLI(t, "issue", "show", "CLI-2", "--json")
	if c != 0 {
		t.Fatalf("show code=%d", c)
	}
	if !strings.Contains(out, `"type": "blocked_by"`) || !strings.Contains(out, `"target": "CLI-1"`) {
		t.Errorf("expected blocked_by relation, got: %s", out)
	}
	out, _, _ = runCLI(t, "issue", "blocked", "--project", "CLI", "--json")
	if !strings.Contains(out, `"key":"CLI-2"`) {
		t.Errorf("expected CLI-2 in blocked list: %s", out)
	}
	// Once blocker is done, CLI-2 should no longer appear.
	if _, _, c := runCLI(t, "issue", "mv", "CLI-1", "done"); c != 0 {
		t.Fatal()
	}
	out, _, _ = runCLI(t, "issue", "blocked", "--project", "CLI", "--json")
	if strings.Contains(out, `"key":"CLI-2"`) {
		t.Errorf("expected CLI-2 not blocked anymore: %s", out)
	}
}

// TestIssueSort checks --sort priority sorts urgent-first.
func TestIssueSort(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "project", "add", "CLI", "--name", "Cliban"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "issue", "add", "--project", "CLI", "--title", "low", "--priority", "low"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "issue", "add", "--project", "CLI", "--title", "urgent", "--priority", "urgent"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "issue", "add", "--project", "CLI", "--title", "med", "--priority", "medium"); c != 0 {
		t.Fatal()
	}
	out, _, _ := runCLI(t, "issue", "ls", "--project", "CLI", "--sort", "priority", "--json")
	lines := strings.Split(strings.TrimRight(out, "\n"), "\n")
	if len(lines) != 3 {
		t.Fatalf("want 3 lines, got %d", len(lines))
	}
	var first map[string]any
	if err := json.Unmarshal([]byte(lines[0]), &first); err != nil {
		t.Fatal(err)
	}
	if first["priority"] != "urgent" {
		t.Errorf("first row priority=%v want urgent", first["priority"])
	}
}

// TestIssueDueDate checks --due and --clear-due.
func TestIssueDueDate(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "project", "add", "CLI", "--name", "Cliban"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "issue", "add", "--project", "CLI", "--title", "x", "--due", "2026-06-01"); c != 0 {
		t.Fatal()
	}
	out, _, _ := runCLI(t, "issue", "show", "CLI-1", "--json")
	if !strings.Contains(out, `"due_date": "2026-06-01"`) {
		t.Errorf("expected due_date 2026-06-01 in: %s", out)
	}
	if _, _, c := runCLI(t, "issue", "edit", "CLI-1", "--clear-due"); c != 0 {
		t.Fatal()
	}
	out, _, _ = runCLI(t, "issue", "show", "CLI-1", "--json")
	if !strings.Contains(out, `"due_date": null`) {
		t.Errorf("expected due_date null after clear, got: %s", out)
	}
}

// TestDescriptionFile verifies --description-file.
func TestDescriptionFile(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "project", "add", "CLI", "--name", "Cliban"); c != 0 {
		t.Fatal()
	}
	tmp := filepath.Join(t.TempDir(), "desc.md")
	if err := os.WriteFile(tmp, []byte("multi\nline\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, _, c := runCLI(t, "issue", "add", "--project", "CLI", "--title", "x", "--description-file", tmp); c != 0 {
		t.Fatal()
	}
	out, _, _ := runCLI(t, "issue", "show", "CLI-1", "--json")
	if !strings.Contains(out, `"description": "multi\nline\n"`) {
		t.Errorf("expected description from file, got: %s", out)
	}
}

// TestLabelLifecycle covers create + attach + filter + remove.
func TestLabelLifecycle(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "project", "add", "CLI", "--name", "Cliban"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "label", "add", "bug", "--project", "CLI"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "issue", "add", "--project", "CLI", "--title", "a", "--label", "bug"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "issue", "add", "--project", "CLI", "--title", "b"); c != 0 {
		t.Fatal()
	}
	out, _, _ := runCLI(t, "issue", "ls", "--project", "CLI", "--label", "bug", "--json")
	if strings.Count(out, `"title":"a"`) != 1 || strings.Contains(out, `"title":"b"`) {
		t.Errorf("label filter expected only 'a': %s", out)
	}
}

// TestAutoArchiveConfig validates that --auto-archive-done-after persists.
func TestAutoArchiveConfig(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "project", "add", "CLI", "--name", "Cliban"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "project", "edit", "CLI", "--auto-archive-done-after", "7d"); c != 0 {
		t.Fatal()
	}
	out, _, _ := runCLI(t, "project", "ls", "--json")
	if !strings.Contains(out, `"auto_archive_done_after_days":7`) {
		t.Errorf("expected days=7 in project ls: %s", out)
	}
}
