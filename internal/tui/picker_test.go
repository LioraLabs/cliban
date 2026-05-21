package tui

import "testing"

func TestPickerModel_FilterNarrowsCandidates(t *testing.T) {
	items := []PickerItem{
		{Key: "AAA-1", Title: "fuzzy finder"},
		{Key: "AAA-2", Title: "unrelated thing"},
	}
	m := NewPickerModel(items)
	m = m.setQuery("fuzz")
	if len(m.visible) != 1 || m.visible[0].Key != "AAA-1" {
		t.Fatalf("filter failed: %+v", m.visible)
	}
}

func TestPickerModel_EmptyQueryShowsAll(t *testing.T) {
	items := []PickerItem{
		{Key: "A", Title: "one"},
		{Key: "B", Title: "two"},
	}
	m := NewPickerModel(items)
	m = m.setQuery("")
	if len(m.visible) != 2 {
		t.Fatalf("expected all 2 items visible, got %d", len(m.visible))
	}
}

func TestPickerModel_NoMatchEmptiesVisible(t *testing.T) {
	items := []PickerItem{
		{Key: "A", Title: "one"},
	}
	m := NewPickerModel(items)
	m = m.setQuery("xyzzy")
	if len(m.visible) != 0 {
		t.Fatalf("expected empty visible, got %d", len(m.visible))
	}
}
