package cli

import (
	"strings"
	"testing"
)

func TestMilestoneLifecycle(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "project", "add", "CLI", "--name", "Cliban"); c != 0 {
		t.Fatal()
	}
	if _, _, c := runCLI(t, "milestone", "add", "--project", "CLI", "--name", "v0.1", "--target", "2026-06-01"); c != 0 {
		t.Fatalf("milestone add code=%d", c)
	}
	out, _, c := runCLI(t, "milestone", "ls", "--project", "CLI", "--json")
	if c != 0 {
		t.Fatalf("ls code=%d", c)
	}
	if !strings.Contains(out, `"name": "v0.1"`) {
		t.Errorf("missing v0.1: %s", out)
	}
	if _, _, c := runCLI(t, "milestone", "edit", "--project", "CLI", "--name", "v0.1", "--status", "completed"); c != 0 {
		t.Fatalf("edit code=%d", c)
	}
	if _, _, c := runCLI(t, "milestone", "rm", "--project", "CLI", "--name", "v0.1"); c != 0 {
		t.Fatalf("rm code=%d", c)
	}
}
