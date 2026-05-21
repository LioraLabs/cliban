package search

import (
	"strings"
	"testing"
)

func TestStripDescription_FenceCollapsed(t *testing.T) {
	input := "before\n```\nsome code here\nmore code\n```\nafter"
	out := stripDescription(input)
	if !strings.Contains(out, "before") {
		t.Errorf("expected output to contain 'before', got %q", out)
	}
	if !strings.Contains(out, "after") {
		t.Errorf("expected output to contain 'after', got %q", out)
	}
	if strings.Contains(out, "some code here") {
		t.Errorf("expected fenced code to be stripped, got %q", out)
	}
	if strings.Contains(out, "more code") {
		t.Errorf("expected fenced code to be stripped, got %q", out)
	}
}

func TestStripDescription_HeadingMarkerRemoved(t *testing.T) {
	input := "## Overview\nbody"
	out := stripDescription(input)
	if strings.Contains(out, "##") {
		t.Errorf("expected heading marker to be removed, got %q", out)
	}
	if !strings.Contains(out, "Overview") {
		t.Errorf("expected heading text to remain, got %q", out)
	}
	if !strings.Contains(out, "body") {
		t.Errorf("expected body to remain, got %q", out)
	}
}

func TestStripDescription_LinkUnwrapped(t *testing.T) {
	input := "see [docs](https://example.com) for info"
	out := stripDescription(input)
	if !strings.Contains(out, "docs") {
		t.Errorf("expected link text 'docs' to remain, got %q", out)
	}
	if strings.Contains(out, "example.com") {
		t.Errorf("expected URL to be stripped, got %q", out)
	}
	if strings.Contains(out, "[") || strings.Contains(out, "](") {
		t.Errorf("expected markdown link syntax to be stripped, got %q", out)
	}
}

func TestStripDescription_LengthCappedAt4K(t *testing.T) {
	input := strings.Repeat("x", 8192)
	out := stripDescription(input)
	if len(out) > 4096 {
		t.Errorf("expected output to be capped at 4096 bytes, got %d", len(out))
	}
}
