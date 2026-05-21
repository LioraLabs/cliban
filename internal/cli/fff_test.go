package cli

import (
	"strings"
	"testing"
)

func TestFFF_BatchModeReturnsJSON(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal("init failed")
	}
	if _, _, c := runCLI(t, "project", "add", "AAA", "--name", "A"); c != 0 {
		t.Fatal("project add failed")
	}
	if _, _, c := runCLI(t, "issue", "add", "--project", "AAA", "--title", "fuzzy ticket"); c != 0 {
		t.Fatal("issue add failed")
	}
	out, stderr, c := runCLI(t, "fff", "fuzzy")
	if c != 0 {
		t.Fatalf("exit %d stderr=%s", c, stderr)
	}
	if !strings.Contains(out, `"key":"AAA-1"`) {
		t.Fatalf("expected AAA-1 in JSON output; got %q", out)
	}
	if !strings.Contains(out, `"score":`) {
		t.Fatalf("expected score field; got %q", out)
	}
}

func TestFFF_BatchModeRequiresQuery(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal("init failed")
	}
	_, _, c := runCLI(t, "fff")
	if c == 0 {
		t.Fatal("expected non-zero exit when no QUERY provided in batch mode")
	}
}

func TestFFF_MutexFlagsError(t *testing.T) {
	if _, _, c := runCLI(t, "init"); c != 0 {
		t.Fatal("init failed")
	}
	_, _, c := runCLI(t, "fff", "--show", "--edit", "x")
	if c == 0 {
		t.Fatal("expected non-zero exit for mutex flag violation")
	}
}
