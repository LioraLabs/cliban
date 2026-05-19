package cli

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestResolveEditor(t *testing.T) {
	os.Setenv("VISUAL", "vis")
	os.Setenv("EDITOR", "ed")
	defer os.Unsetenv("VISUAL")
	defer os.Unsetenv("EDITOR")
	if got := ResolveEditor(); got != "vis" {
		t.Errorf("VISUAL precedence broken: %q", got)
	}
	os.Unsetenv("VISUAL")
	if got := ResolveEditor(); got != "ed" {
		t.Errorf("EDITOR fallback broken: %q", got)
	}
	os.Unsetenv("EDITOR")
	if got := ResolveEditor(); got != "vi" {
		t.Errorf("vi default broken: %q", got)
	}
}

func TestRunEditorWritesBack(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "buffer.md")
	if err := os.WriteFile(path, []byte("hello\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	os.Setenv("EDITOR", "sh -c 'echo edited >> $1' --")
	defer os.Unsetenv("EDITOR")
	if err := RunEditor(path); err != nil {
		t.Fatalf("RunEditor: %v", err)
	}
	data, _ := os.ReadFile(path)
	if !strings.Contains(string(data), "edited") {
		t.Errorf("editor didn't append: %q", string(data))
	}
}
