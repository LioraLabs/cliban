package cli

import (
	"os"
	"strings"
	"testing"
)

func TestIssueAddOpensEditor(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "project", "add", "CLI", "--name", "Cliban"); c != 0 {
		t.Fatal()
	}
	origVisual := os.Getenv("VISUAL")
	os.Unsetenv("VISUAL")
	os.Setenv("EDITOR", `sh -c '/usr/bin/printf "---\ntitle: From editor\nstatus: backlog\npriority: high\nmilestone:\nparent:\n---\nbody\n" > $1' --`)
	os.Setenv("CLIBAN_FORCE_TTY", "1")
	defer func() {
		if origVisual != "" {
			os.Setenv("VISUAL", origVisual)
		}
	}()
	defer os.Unsetenv("EDITOR")
	defer os.Unsetenv("CLIBAN_FORCE_TTY")
	_, _, c := runCLI(t, "issue", "add", "--project", "CLI")
	if c != 0 {
		t.Fatalf("issue add via editor code=%d", c)
	}
	out, _, _ := runCLI(t, "issue", "show", "CLI-1", "--json")
	if !strings.Contains(out, `"title": "From editor"`) {
		t.Errorf("editor result not applied: %s", out)
	}
	if !strings.Contains(out, `"priority": "high"`) {
		t.Errorf("priority lost: %s", out)
	}
}

func TestIssueAddNoEditorFlagPreventsEditor(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "project", "add", "CLI", "--name", "Cliban"); c != 0 {
		t.Fatal()
	}
	_, _, c := runCLI(t, "issue", "add", "--project", "CLI", "--no-editor")
	if c != 2 {
		t.Errorf("want exit code 2 (validation), got %d", c)
	}
}
