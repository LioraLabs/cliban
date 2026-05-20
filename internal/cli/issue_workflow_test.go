package cli

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestIssueCurrent_BranchMatches(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal("init")
	}
	if _, _, c := runCLI(t, "project", "add", "CUR", "--name", "Current"); c != 0 {
		t.Fatal("project add")
	}
	if _, _, c := runCLI(t, "issue", "add", "--project", "CUR", "--title", "hello world"); c != 0 {
		t.Fatal("issue add")
	}
	os.Setenv("CLIBAN_CURRENT_BRANCH_OVERRIDE", "cur-1-hello-world")
	t.Cleanup(func() { os.Unsetenv("CLIBAN_CURRENT_BRANCH_OVERRIDE") })

	out, _, c := runCLI(t, "issue", "current", "--json")
	if c != 0 {
		t.Fatalf("current code=%d out=%s", c, out)
	}
	var m map[string]any
	if err := json.Unmarshal([]byte(out), &m); err != nil {
		t.Fatalf("parse: %v: %s", err, out)
	}
	if m["key"] != "CUR-1" {
		t.Fatalf("expected CUR-1, got %v", m["key"])
	}
}

func TestIssueCurrent_NoBranchMatch(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal("init")
	}
	if _, _, c := runCLI(t, "project", "add", "NCB", "--name", "NoBranch"); c != 0 {
		t.Fatal("project add")
	}
	os.Setenv("CLIBAN_CURRENT_BRANCH_OVERRIDE", "main")
	t.Cleanup(func() { os.Unsetenv("CLIBAN_CURRENT_BRANCH_OVERRIDE") })

	_, errOut, c := runCLI(t, "issue", "current", "--json")
	if c == 0 {
		t.Fatal("expected non-zero exit for unmatched branch")
	}
	if !strings.Contains(errOut, "no issue found for current branch") {
		t.Fatalf("unexpected error: %s", errOut)
	}
}

func TestIssueTick_HappyPath(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal("init")
	}
	if _, _, c := runCLI(t, "project", "add", "TCK", "--name", "Tick"); c != 0 {
		t.Fatal("project add")
	}
	body := "## Plan\n\n### Task 1: foo\n\n- [ ] **Step 1: a**\n- [ ] **Step 2: b**\n"
	writeIssueDesc(t, "TCK", "tick-test", body)
	if _, _, c := runCLI(t, "issue", "tick", "TCK-1", "--task", "1", "--step", "2", "--json"); c != 0 {
		t.Fatalf("tick code=%d", c)
	}
	out, _, c := runCLI(t, "issue", "show", "TCK-1", "--section", "plan")
	if c != 0 {
		t.Fatalf("show code=%d", c)
	}
	if !strings.Contains(out, "- [x] **Step 2: b**") {
		t.Fatalf("expected step 2 ticked; description was:\n%s", out)
	}
}

func TestIssueTick_AlreadyChecked(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal("init")
	}
	if _, _, c := runCLI(t, "project", "add", "TCK2", "--name", "Tick2"); c != 0 {
		t.Fatal("project add")
	}
	body := "## Plan\n\n### Task 1: foo\n\n- [x] **Step 1: a**\n"
	writeIssueDesc(t, "TCK2", "already", body)
	_, errOut, c := runCLI(t, "issue", "tick", "TCK2-1", "--task", "1", "--step", "1")
	if c == 0 {
		t.Fatal("expected non-zero exit for already-checked step")
	}
	if c != 2 {
		t.Fatalf("expected exit code 2 (validation), got %d: %s", c, errOut)
	}
	if !strings.Contains(errOut, "already checked") {
		t.Fatalf("expected already-checked error, got %q", errOut)
	}
}

func TestIssueTick_NoPlanSection(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal("init")
	}
	if _, _, c := runCLI(t, "project", "add", "TCK3", "--name", "Tick3"); c != 0 {
		t.Fatal("project add")
	}
	writeIssueDesc(t, "TCK3", "no plan", "just a description with no plan section\n")
	_, errOut, c := runCLI(t, "issue", "tick", "TCK3-1", "--task", "1", "--step", "1")
	if c == 0 {
		t.Fatal("expected non-zero exit for missing plan section")
	}
	if !strings.Contains(errOut, "no ## Plan section") {
		t.Fatalf("expected no-plan error, got %q", errOut)
	}
}

func TestIssueLog_AppendsEntry(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal("init")
	}
	if _, _, c := runCLI(t, "project", "add", "LOG", "--name", "Log"); c != 0 {
		t.Fatal("project add")
	}
	writeIssueDesc(t, "LOG", "logtest", "## Spec\n\nx\n")
	if _, _, c := runCLI(t, "issue", "log", "LOG-1", "first entry", "--json"); c != 0 {
		t.Fatalf("log 1 code=%d", c)
	}
	if _, _, c := runCLI(t, "issue", "log", "LOG-1", "second entry", "--json"); c != 0 {
		t.Fatalf("log 2 code=%d", c)
	}
	out, _, c := runCLI(t, "issue", "show", "LOG-1", "--section", "activity")
	if c != 0 {
		t.Fatalf("show code=%d", c)
	}
	if !strings.Contains(out, "first entry") || !strings.Contains(out, "second entry") {
		t.Fatalf("expected both entries; got:\n%s", out)
	}
	if strings.Index(out, "first entry") > strings.Index(out, "second entry") {
		t.Fatalf("entries should be in chronological order:\n%s", out)
	}
}

func TestIssueLog_CreatesSectionIfAbsent(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal("init")
	}
	if _, _, c := runCLI(t, "project", "add", "LOG2", "--name", "Log2"); c != 0 {
		t.Fatal("project add")
	}
	writeIssueDesc(t, "LOG2", "noactivity", "")
	if _, _, c := runCLI(t, "issue", "log", "LOG2-1", "hello"); c != 0 {
		t.Fatalf("log code=%d", c)
	}
	out, _, c := runCLI(t, "issue", "show", "LOG2-1", "--section", "activity")
	if c != 0 {
		t.Fatalf("show code=%d", c)
	}
	if !strings.Contains(out, "hello") {
		t.Fatalf("expected 'hello' in section, got:\n%s", out)
	}
}

func TestIssueLog_MessageFile(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal("init")
	}
	if _, _, c := runCLI(t, "project", "add", "LOG3", "--name", "Log3"); c != 0 {
		t.Fatal("project add")
	}
	writeIssueDesc(t, "LOG3", "filetest", "")
	msgPath := filepath.Join(t.TempDir(), "msg.txt")
	if err := os.WriteFile(msgPath, []byte("multi-line\nmessage from file"), 0o600); err != nil {
		t.Fatalf("write msg: %v", err)
	}
	if _, _, c := runCLI(t, "issue", "log", "LOG3-1", "--message-file", msgPath); c != 0 {
		t.Fatalf("log code=%d", c)
	}
	out, _, c := runCLI(t, "issue", "show", "LOG3-1", "--section", "activity")
	if c != 0 {
		t.Fatalf("show code=%d", c)
	}
	if !strings.Contains(out, "multi-line") {
		t.Fatalf("expected file message, got:\n%s", out)
	}
}

func TestIssuePromote_SubIssue(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal("init")
	}
	if _, _, c := runCLI(t, "project", "add", "PRM", "--name", "Promote"); c != 0 {
		t.Fatal("project add")
	}
	body := "## Plan\n\n### Task 1: foo\n\n- [ ] **Step 1: do thing**\n- [ ] **Step 2: bigger thing**\n"
	writeIssueDesc(t, "PRM", "parent", body)
	out, _, c := runCLI(t, "issue", "promote", "PRM-1",
		"--task", "1", "--step", "2", "--title", "Bigger thing as own issue",
		"--as", "sub-issue", "--json")
	if c != 0 {
		t.Fatalf("promote code=%d out=%s", c, out)
	}
	var m map[string]any
	if err := json.Unmarshal([]byte(out), &m); err != nil {
		t.Fatalf("parse: %v: %s", err, out)
	}
	if got, _ := m["new_key"].(string); got != "PRM-2" {
		t.Fatalf("expected new_key PRM-2, got %v", m["new_key"])
	}
	planOut, _, c := runCLI(t, "issue", "show", "PRM-1", "--section", "plan")
	if c != 0 {
		t.Fatalf("show plan code=%d", c)
	}
	if !strings.Contains(planOut, "→ PRM-2") {
		t.Fatalf("expected step line rewritten with arrow; got:\n%s", planOut)
	}
	subOut, _, c := runCLI(t, "issue", "show", "PRM-2", "--json")
	if c != 0 {
		t.Fatalf("show sub code=%d", c)
	}
	if !strings.Contains(subOut, `"parent": "PRM-1"`) {
		t.Fatalf("expected PRM-2 to be sub-issue of PRM-1; got:\n%s", subOut)
	}
}

func TestIssuePromote_Related(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal("init")
	}
	if _, _, c := runCLI(t, "project", "add", "REL", "--name", "Rel"); c != 0 {
		t.Fatal("project add")
	}
	writeIssueDesc(t, "REL", "parent",
		"## Plan\n\n### Task 1: foo\n\n- [ ] **Step 1: do thing**\n")
	out, _, c := runCLI(t, "issue", "promote", "REL-1",
		"--task", "1", "--step", "1", "--title", "Related work",
		"--as", "related", "--json")
	if c != 0 {
		t.Fatalf("promote code=%d", c)
	}
	var m map[string]any
	if err := json.Unmarshal([]byte(out), &m); err != nil {
		t.Fatalf("parse: %v", err)
	}
	newKey, _ := m["new_key"].(string)
	relOut, _, c := runCLI(t, "issue", "show", newKey, "--json")
	if c != 0 {
		t.Fatalf("show new code=%d", c)
	}
	if !strings.Contains(relOut, `"type": "related_to"`) || !strings.Contains(relOut, `"target": "REL-1"`) {
		t.Fatalf("expected related_to relation to REL-1; got:\n%s", relOut)
	}
}

func TestIssuePromote_InvalidAs(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal("init")
	}
	if _, _, c := runCLI(t, "project", "add", "PI", "--name", "PromInv"); c != 0 {
		t.Fatal("project add")
	}
	writeIssueDesc(t, "PI", "x",
		"## Plan\n\n### Task 1: foo\n\n- [ ] **Step 1: a**\n")
	_, errOut, c := runCLI(t, "issue", "promote", "PI-1",
		"--task", "1", "--step", "1", "--title", "x", "--as", "bogus")
	if c == 0 {
		t.Fatal("expected non-zero exit for invalid --as")
	}
	if !strings.Contains(errOut, "invalid --as") {
		t.Fatalf("expected invalid-as error, got %q", errOut)
	}
}
