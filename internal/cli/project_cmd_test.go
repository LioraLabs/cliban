package cli

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// testDBPath returns (and caches) a stable per-test DB path so that multiple
// runCLI calls within the same test all share the same database.
var testDBPaths = map[string]string{}

func testDBPath(t *testing.T) string {
	t.Helper()
	if p, ok := testDBPaths[t.Name()]; ok {
		return p
	}
	p := filepath.Join(t.TempDir(), "test.db")
	testDBPaths[t.Name()] = p
	t.Cleanup(func() { delete(testDBPaths, t.Name()) })
	return p
}

func runCLI(t *testing.T, args ...string) (stdout, stderr string, code int) {
	t.Helper()
	dbPath := testDBPath(t)
	os.Setenv("CLIBAN_DB", dbPath)
	t.Cleanup(func() { os.Unsetenv("CLIBAN_DB") })

	// Reset globals so a fresh Root sees no stale --db.
	G = &Globals{}

	root := NewRoot()
	var ob, eb bytes.Buffer
	root.SetOut(&ob)
	root.SetErr(&eb)
	root.SetArgs(args)
	if err := root.Execute(); err != nil {
		return ob.String(), eb.String() + err.Error(), ExitCodeFor(err)
	}
	return ob.String(), eb.String(), 0
}

func TestProjectAddAndList(t *testing.T) {
	if _, _, code := runCLI(t, "init"); code != 0 {
		t.Fatal("init failed")
	}
	if _, _, code := runCLI(t, "project", "add", "CLI", "--name", "Cliban", "--description", "kanban"); code != 0 {
		t.Fatalf("project add code=%d", code)
	}
	out, _, code := runCLI(t, "project", "ls", "--json")
	if code != 0 {
		t.Fatalf("ls code=%d", code)
	}
	if !strings.Contains(out, `"key": "CLI"`) {
		t.Errorf("missing CLI in output: %s", out)
	}
	var arr []map[string]any
	dec := json.NewDecoder(strings.NewReader(out))
	for dec.More() {
		var v map[string]any
		if err := dec.Decode(&v); err != nil {
			t.Fatalf("decode: %v", err)
		}
		arr = append(arr, v)
	}
	if len(arr) != 1 {
		t.Errorf("len=%d want 1", len(arr))
	}
}

func TestProjectShowNotFound(t *testing.T) {
	if _, _, code := runCLI(t, "init"); code != 0 {
		t.Fatal()
	}
	_, _, code := runCLI(t, "project", "show", "NOPE")
	if code != 1 {
		t.Errorf("want exit code 1 (not-found), got %d", code)
	}
}
