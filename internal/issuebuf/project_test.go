package issuebuf

import (
	"strings"
	"testing"
)

func TestProjectBufferRoundtrip(t *testing.T) {
	src := "# Editing project CLI — lines above the first '---' are ignored.\n" +
		"---\n" +
		"name: Cliban\n" +
		"---\n" +
		"Self-hosted kanban CLI.\n"
	got, err := ParseProjectBuffer(src)
	if err != nil {
		t.Fatalf("ParseProjectBuffer: %v", err)
	}
	if got.Name != "Cliban" {
		t.Errorf("Name=%q want Cliban", got.Name)
	}
	if !strings.Contains(got.Description, "Self-hosted kanban CLI.") {
		t.Errorf("body lost: %q", got.Description)
	}
}

func TestProjectBufferSerializeParsesBack(t *testing.T) {
	b := ProjectBuffer{
		Header:      "# Editing project CLI",
		Name:        "Cliban",
		Description: "Line one.\n\nLine two.\n",
	}
	got, err := ParseProjectBuffer(b.Serialize())
	if err != nil {
		t.Fatalf("ParseProjectBuffer(Serialize()): %v", err)
	}
	if got.Name != b.Name {
		t.Errorf("Name=%q want %q", got.Name, b.Name)
	}
	if got.Description != b.Description {
		t.Errorf("Description=%q want %q", got.Description, b.Description)
	}
}

func TestProjectBufferRequiresName(t *testing.T) {
	src := "---\nname:\n---\nbody\n"
	if _, err := ParseProjectBuffer(src); err == nil {
		t.Error("expected error when name is empty")
	}
}
