package cli

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestMilestoneAdd_DescriptionFile(t *testing.T) {
	dir := t.TempDir()
	descPath := filepath.Join(dir, "desc.md")
	body := "## Spec\n\nmilestone goal here\n"
	if err := os.WriteFile(descPath, []byte(body), 0o600); err != nil {
		t.Fatalf("write desc: %v", err)
	}
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal("init")
	}
	if _, _, c := runCLI(t, "project", "add", "MS", "--name", "Milestones"); c != 0 {
		t.Fatal("project add")
	}
	if _, _, c := runCLI(t, "milestone", "add", "--project", "MS", "--name", "v0.1", "--description-file", descPath); c != 0 {
		t.Fatalf("milestone add code=%d", c)
	}
	out, _, c := runCLI(t, "milestone", "show", "v0.1", "--project", "MS", "--json")
	if c != 0 {
		t.Fatalf("show code=%d out=%s", c, out)
	}
	var m map[string]any
	if err := json.Unmarshal([]byte(out), &m); err != nil {
		t.Fatalf("parse json: %v: %s", err, out)
	}
	if got := m["description"]; got != body {
		t.Fatalf("description mismatch:\n got=%v\nwant=%q", got, body)
	}
}

func TestMilestoneEdit_DescriptionFile(t *testing.T) {
	dir := t.TempDir()
	descPath := filepath.Join(dir, "desc.md")
	body := "updated body\n"
	if err := os.WriteFile(descPath, []byte(body), 0o600); err != nil {
		t.Fatalf("write desc: %v", err)
	}
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal("init")
	}
	if _, _, c := runCLI(t, "project", "add", "ME", "--name", "MilestoneEdit"); c != 0 {
		t.Fatal("project add")
	}
	if _, _, c := runCLI(t, "milestone", "add", "--project", "ME", "--name", "v1", "--description", "initial"); c != 0 {
		t.Fatal("milestone add")
	}
	if _, _, c := runCLI(t, "milestone", "edit", "--project", "ME", "--name", "v1", "--description-file", descPath); c != 0 {
		t.Fatalf("milestone edit code=%d", c)
	}
	out, _, c := runCLI(t, "milestone", "show", "v1", "--project", "ME", "--json")
	if c != 0 {
		t.Fatalf("show code=%d", c)
	}
	var m map[string]any
	if err := json.Unmarshal([]byte(out), &m); err != nil {
		t.Fatalf("parse json: %v", err)
	}
	if got := m["description"]; got != body {
		t.Fatalf("description mismatch: got=%v want=%q", got, body)
	}
}

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
	if !strings.Contains(out, `"name":"v0.1"`) {
		t.Errorf("missing v0.1 in NDJSON output: %s", out)
	}
	if _, _, c := runCLI(t, "milestone", "edit", "--project", "CLI", "--name", "v0.1", "--status", "completed"); c != 0 {
		t.Fatalf("edit code=%d", c)
	}
	if _, _, c := runCLI(t, "milestone", "rm", "--project", "CLI", "--name", "v0.1"); c != 0 {
		t.Fatalf("rm code=%d", c)
	}
}
