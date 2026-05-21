package cli

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestIssueArchiveLifecycle(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "project", "add", "CLI", "--name", "Cliban"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "issue", "add", "--project", "CLI", "--title", "todo"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "issue", "add", "--project", "CLI", "--title", "wontfix"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "issue", "archive", "CLI-2"); c != 0 {
		t.Fatalf("archive code=%d", c)
	}
	out, _, _ := runCLI(t, "issue", "ls", "--project", "CLI", "--json")
	if count := strings.Count(out, `"key":`); count != 1 {
		t.Errorf("default ls returned %d, want 1: %s", count, out)
	}
	out, _, _ = runCLI(t, "issue", "ls", "--project", "CLI", "--archived", "--json")
	if count := strings.Count(out, `"key":`); count != 2 {
		t.Errorf("--archived ls returned %d, want 2: %s", count, out)
	}
	if _, _, c := runCLI(t, "issue", "unarchive", "CLI-2"); c != 0 {
		t.Fatalf("unarchive code=%d", c)
	}
	out, _, _ = runCLI(t, "issue", "ls", "--project", "CLI", "--json")
	if count := strings.Count(out, `"key":`); count != 2 {
		t.Errorf("after unarchive ls returned %d, want 2: %s", count, out)
	}
}

func TestIssueArchiveDone(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "project", "add", "CLI", "--name", "Cliban"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "issue", "add", "--project", "CLI", "--title", "a"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "issue", "add", "--project", "CLI", "--title", "b"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "issue", "mv", "CLI-1", "done"); c != 0 {
		t.Fatal()
	}
	out, _, c := runCLI(t, "issue", "archive-done", "--project", "CLI", "--json")
	if c != 0 {
		t.Fatalf("archive-done code=%d", c)
	}
	if !strings.Contains(out, `"archived": 1`) {
		t.Errorf("expected archived count 1: %s", out)
	}
	out, _, _ = runCLI(t, "issue", "ls", "--project", "CLI", "--json")
	if !strings.Contains(out, `"title":"b"`) || strings.Contains(out, `"title":"a"`) {
		t.Errorf("after archive-done expected only 'b': %s", out)
	}
}

func TestIssueLifecycle(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "project", "add", "CLI", "--name", "Cliban"); c != 0 {
		t.Fatal()
	}

	out, _, c := runCLI(t, "issue", "add", "--project", "CLI", "--title", "First", "--priority", "high", "--json")
	if c != 0 {
		t.Fatalf("issue add code=%d out=%s", c, out)
	}
	var first map[string]any
	if err := json.Unmarshal([]byte(out), &first); err != nil {
		t.Fatalf("decode: %v\n%s", err, out)
	}
	if first["key"] != "CLI-1" {
		t.Errorf("first key = %v want CLI-1", first["key"])
	}

	if _, _, c := runCLI(t, "issue", "add", "--project", "CLI", "--title", "Sub", "--parent", "CLI-1"); c != 0 {
		t.Fatalf("subissue code=%d", c)
	}

	if _, _, c := runCLI(t, "issue", "mv", "CLI-1", "in-progress"); c != 0 {
		t.Fatalf("mv code=%d", c)
	}

	if _, _, c := runCLI(t, "issue", "edit", "CLI-1", "--description", "updated"); c != 0 {
		t.Fatalf("edit code=%d", c)
	}

	out, _, _ = runCLI(t, "issue", "show", "CLI-1", "--json")
	if !strings.Contains(out, `"status": "in-progress"`) {
		t.Errorf("show missing status: %s", out)
	}
	if !strings.Contains(out, `"description": "updated"`) {
		t.Errorf("show missing description: %s", out)
	}

	out, _, _ = runCLI(t, "issue", "ls", "--project", "CLI", "--json")
	count := strings.Count(out, `"key":`)
	if count != 2 {
		t.Errorf("ls returned %d issues, want 2: %s", count, out)
	}

	if _, _, c := runCLI(t, "issue", "rm", "CLI-1"); c != 0 {
		t.Fatalf("rm code=%d", c)
	}
	out, _, _ = runCLI(t, "issue", "ls", "--project", "CLI", "--json")
	if strings.Count(out, `"key":`) != 0 {
		t.Errorf("issues remained: %s", out)
	}
}

func TestIssueLs_UpdatedSince_Duration(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal("init")
	}
	if _, _, c := runCLI(t, "project", "add", "USN", "--name", "UpdSince"); c != 0 {
		t.Fatal("project add")
	}
	if _, _, c := runCLI(t, "issue", "add", "--project", "USN", "--title", "old issue"); c != 0 {
		t.Fatal("old add")
	}
	time.Sleep(1100 * time.Millisecond)
	if _, _, c := runCLI(t, "issue", "add", "--project", "USN", "--title", "fresh issue"); c != 0 {
		t.Fatal("fresh add")
	}
	out, _, c := runCLI(t, "issue", "ls", "--project", "USN", "--updated-since", "1s", "--json")
	if c != 0 {
		t.Fatalf("ls code=%d out=%s", c, out)
	}
	var titles []string
	for _, line := range strings.Split(strings.TrimSpace(out), "\n") {
		if line == "" {
			continue
		}
		var m map[string]any
		if err := json.Unmarshal([]byte(line), &m); err != nil {
			t.Fatalf("parse: %v", err)
		}
		titles = append(titles, m["title"].(string))
	}
	if len(titles) != 1 || titles[0] != "fresh issue" {
		t.Fatalf("expected exactly fresh issue; got %v", titles)
	}
}

func TestIssueLs_UpdatedSince_Timestamp(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal("init")
	}
	if _, _, c := runCLI(t, "project", "add", "UST", "--name", "UpdSinceTs"); c != 0 {
		t.Fatal("project add")
	}
	if _, _, c := runCLI(t, "issue", "add", "--project", "UST", "--title", "issue 1"); c != 0 {
		t.Fatal("add 1")
	}
	cutoff := time.Now().UTC().Format(time.RFC3339)
	time.Sleep(1100 * time.Millisecond)
	if _, _, c := runCLI(t, "issue", "add", "--project", "UST", "--title", "issue 2"); c != 0 {
		t.Fatal("add 2")
	}
	out, _, c := runCLI(t, "issue", "ls", "--project", "UST", "--updated-since", cutoff, "--json")
	if c != 0 {
		t.Fatalf("ls code=%d", c)
	}
	if !strings.Contains(out, "issue 2") || strings.Contains(out, "issue 1") {
		t.Fatalf("expected only issue 2; got %s", out)
	}
}

// writeIssueDesc creates a tempfile with the given body and adds an issue
// whose description is read from that file. Returns the resulting issue key.
func writeIssueDesc(t *testing.T, project, title, body string) {
	t.Helper()
	p := filepath.Join(t.TempDir(), "desc.md")
	if err := os.WriteFile(p, []byte(body), 0o600); err != nil {
		t.Fatalf("write desc: %v", err)
	}
	if _, _, c := runCLI(t, "issue", "add", "--project", project, "--title", title, "--description-file", p); c != 0 {
		t.Fatalf("issue add code=%d", c)
	}
}

func TestIssueShow_Section_Spec(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal("init")
	}
	if _, _, c := runCLI(t, "project", "add", "SEC", "--name", "Sect"); c != 0 {
		t.Fatal("project add")
	}
	body := "## Spec\n\nthe spec body\n\n## Plan\n\n### Task 1: foo\n\n- [ ] **Step 1: a**\n"
	writeIssueDesc(t, "SEC", "test", body)
	out, _, c := runCLI(t, "issue", "show", "SEC-1", "--section", "spec")
	if c != 0 {
		t.Fatalf("show code=%d out=%s", c, out)
	}
	if !strings.Contains(out, "the spec body") {
		t.Fatalf("expected spec body in output; got %q", out)
	}
	if strings.Contains(out, "the spec body\n\n## Plan") {
		t.Fatalf("section output should stop at next H2; got %q", out)
	}
}

func TestIssueShow_Section_NotFound(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal("init")
	}
	if _, _, c := runCLI(t, "project", "add", "SEC2", "--name", "Sect2"); c != 0 {
		t.Fatal("project add")
	}
	writeIssueDesc(t, "SEC2", "no plan", "## Spec\n\njust spec\n")
	_, errOut, c := runCLI(t, "issue", "show", "SEC2-1", "--section", "plan")
	if c == 0 {
		t.Fatal("expected non-zero exit for missing plan section")
	}
	if !strings.Contains(errOut, "no ## Plan section") {
		t.Fatalf("expected no-plan error; got %q", errOut)
	}
}

func TestIssueLs_SearchReturnsScoredJSON(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal("init failed")
	}
	if _, _, c := runCLI(t, "project", "add", "AAA", "--name", "A"); c != 0 {
		t.Fatal("project add failed")
	}
	if _, _, c := runCLI(t, "issue", "add", "--project", "AAA", "--title", "fuzzy ticket finder"); c != 0 {
		t.Fatal("issue add failed")
	}
	if _, _, c := runCLI(t, "issue", "add", "--project", "AAA", "--title", "decoy"); c != 0 {
		t.Fatal("decoy add failed")
	}
	out, _, c := runCLI(t, "issue", "ls", "--project", "AAA", "--search", "fuzzy", "--json")
	if c != 0 {
		t.Fatalf("ls --search code=%d out=%s", c, out)
	}
	if !strings.Contains(out, `"key":"AAA-1"`) {
		t.Fatalf("expected AAA-1 in output: %s", out)
	}
	if !strings.Contains(out, `"score":`) {
		t.Fatalf("expected score field in NDJSON: %s", out)
	}
	// The decoy ranks lower or doesn't appear — verify AAA-1 comes BEFORE AAA-2 in the output.
	iAAA1 := strings.Index(out, `"key":"AAA-1"`)
	iAAA2 := strings.Index(out, `"key":"AAA-2"`)
	if iAAA2 >= 0 && iAAA1 > iAAA2 {
		t.Fatalf("AAA-1 should rank above AAA-2: out=%s", out)
	}
}

func TestIssueLs_EmptySearchErrors(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal("init failed")
	}
	_, _, c := runCLI(t, "issue", "ls", "--search", "  ")
	if c == 0 {
		t.Fatal("expected non-zero exit for whitespace-only --search")
	}
}

func TestIssueShow_Pager_FallbackPlainOutput(t *testing.T) {
	// Use 'cat' as PAGER so the pipe is non-interactive and ends up on stdout.
	os.Setenv("PAGER", "cat")
	t.Cleanup(func() { os.Unsetenv("PAGER") })
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal("init")
	}
	if _, _, c := runCLI(t, "project", "add", "PGR", "--name", "Pager"); c != 0 {
		t.Fatal("project add")
	}
	writeIssueDesc(t, "PGR", "showme", "## Spec\n\nbody\n")
	out, _, c := runCLI(t, "issue", "show", "PGR-1", "--pager")
	if c != 0 {
		t.Fatalf("show code=%d", c)
	}
	if !strings.Contains(out, "body") {
		t.Fatalf("expected body in piped output, got:\n%s", out)
	}
}

func TestIssueLs_SearchPlusSortWarns(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal("init failed")
	}
	if _, _, c := runCLI(t, "project", "add", "AAA", "--name", "A"); c != 0 {
		t.Fatal("project add failed")
	}
	if _, _, c := runCLI(t, "issue", "add", "--project", "AAA", "--title", "fuzzy"); c != 0 {
		t.Fatal("issue add failed")
	}
	stdout, stderr, code := runCLI(t, "issue", "ls", "--project", "AAA", "--search", "fuzzy", "--sort", "priority")
	if code != 0 {
		t.Fatalf("exit code = %d, stderr=%s", code, stderr)
	}
	if !strings.Contains(stderr, "ignored") {
		t.Fatalf("expected stderr to mention --sort being ignored; got %q", stderr)
	}
	if !strings.Contains(stdout, "AAA-1") {
		t.Fatalf("expected result in stdout despite warning; got %q", stdout)
	}
}
