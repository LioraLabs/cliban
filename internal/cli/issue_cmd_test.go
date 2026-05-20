package cli

import (
	"encoding/json"
	"strings"
	"testing"
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
