package store

import (
	"reflect"
	"testing"

	"github.com/alex/cliban/internal/domain"
)

func TestLabelsForIssues(t *testing.T) {
	s := newTestStore(t)
	if _, err := s.CreateProject("AAA", "A", ""); err != nil {
		t.Fatalf("CreateProject: %v", err)
	}
	i1, err := s.CreateIssue(CreateIssueParams{ProjectKey: "AAA", Title: "one"})
	if err != nil {
		t.Fatalf("CreateIssue one: %v", err)
	}
	i2, err := s.CreateIssue(CreateIssueParams{ProjectKey: "AAA", Title: "two"})
	if err != nil {
		t.Fatalf("CreateIssue two: %v", err)
	}
	// Third issue with no labels — must be absent from the result map.
	i3, err := s.CreateIssue(CreateIssueParams{ProjectKey: "AAA", Title: "three"})
	if err != nil {
		t.Fatalf("CreateIssue three: %v", err)
	}

	k1 := domain.IssueKey{Project: "AAA", Seq: i1.Seq}
	k2 := domain.IssueKey{Project: "AAA", Seq: i2.Seq}
	if err := s.AttachLabel(k1, "bug"); err != nil {
		t.Fatalf("AttachLabel bug→i1: %v", err)
	}
	if err := s.AttachLabel(k1, "feature"); err != nil {
		t.Fatalf("AttachLabel feature→i1: %v", err)
	}
	if err := s.AttachLabel(k2, "feature"); err != nil {
		t.Fatalf("AttachLabel feature→i2: %v", err)
	}

	got, err := s.LabelsForIssues([]int64{i1.ID, i2.ID, i3.ID})
	if err != nil {
		t.Fatalf("LabelsForIssues: %v", err)
	}
	if !reflect.DeepEqual(got[i1.ID], []string{"bug", "feature"}) {
		t.Fatalf("i1 labels: got %v want [bug feature]", got[i1.ID])
	}
	if !reflect.DeepEqual(got[i2.ID], []string{"feature"}) {
		t.Fatalf("i2 labels: got %v want [feature]", got[i2.ID])
	}
	if _, ok := got[i3.ID]; ok {
		t.Fatalf("i3 has no labels but appeared in map: %v", got[i3.ID])
	}
}

func TestLabelsForIssues_EmptyInput(t *testing.T) {
	s := newTestStore(t)
	got, err := s.LabelsForIssues(nil)
	if err != nil {
		t.Fatalf("LabelsForIssues(nil): %v", err)
	}
	if len(got) != 0 {
		t.Fatalf("empty input must yield empty map, got %v", got)
	}
	got, err = s.LabelsForIssues([]int64{})
	if err != nil {
		t.Fatalf("LabelsForIssues([]): %v", err)
	}
	if len(got) != 0 {
		t.Fatalf("empty slice must yield empty map, got %v", got)
	}
}
