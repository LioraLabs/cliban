package issuebuf

import (
	"strings"
	"testing"
)

func TestMilestoneBufferRoundtrip(t *testing.T) {
	src := "# Creating milestone in CLI — lines above the first '---' are ignored.\n" +
		"---\n" +
		"name:   v0.1\n" +
		"target: 2026-06-01\n" +
		"status: open\n" +
		"---\n" +
		"First release. Targets the storage layer.\n"
	got, err := ParseMilestoneBuffer(src)
	if err != nil {
		t.Fatalf("ParseMilestoneBuffer: %v", err)
	}
	if got.Name != "v0.1" || got.Target != "2026-06-01" || got.Status != "open" {
		t.Errorf("unexpected: %+v", got)
	}
	if !strings.Contains(got.Description, "First release") {
		t.Errorf("body lost: %q", got.Description)
	}
}

func TestMilestoneBufferRequiresName(t *testing.T) {
	src := "---\nname:\ntarget: 2026-06-01\nstatus: open\n---\n\n"
	if _, err := ParseMilestoneBuffer(src); err == nil {
		t.Error("expected error when name is empty")
	}
}

func TestMilestoneBufferInvalidStatus(t *testing.T) {
	src := "---\nname: v0.1\ntarget:\nstatus: nope\n---\n\n"
	if _, err := ParseMilestoneBuffer(src); err == nil {
		t.Error("expected error for invalid status")
	}
}
