package search_test

import (
	"context"
	"fmt"
	"path/filepath"
	"testing"
	"time"

	"github.com/alex/cliban/internal/domain"
	"github.com/alex/cliban/internal/search"
	"github.com/alex/cliban/internal/store"
)

// newTestStore opens an in-memory-ish (tempdir) SQLite store with the schema
// migrated. Used by search_test.go to drive the public API end-to-end without
// touching internal-package helpers.
func newTestStore(t testing.TB) *store.Store {
	t.Helper()
	s, err := store.Open(filepath.Join(t.TempDir(), "test.db"))
	if err != nil {
		t.Fatalf("Open: %v", err)
	}
	t.Cleanup(func() { _ = s.Close() })
	if err := s.Migrate(); err != nil {
		t.Fatalf("Migrate: %v", err)
	}
	return s
}

func mustCreateProject(t testing.TB, s *store.Store, key, name string) {
	t.Helper()
	if _, err := s.CreateProject(key, name, ""); err != nil {
		t.Fatalf("CreateProject %s: %v", key, err)
	}
}

func mustCreateIssue(t testing.TB, s *store.Store, projectKey, title string) *domain.Issue {
	t.Helper()
	iss, err := s.CreateIssue(store.CreateIssueParams{ProjectKey: projectKey, Title: title})
	if err != nil {
		t.Fatalf("CreateIssue %q: %v", title, err)
	}
	return iss
}

func mustUpdateDescription(t testing.TB, s *store.Store, projectKey string, seq int64, desc string) {
	t.Helper()
	k := domain.IssueKey{Project: projectKey, Seq: seq}
	if err := s.UpdateIssue(k, store.UpdateIssueParams{Description: &desc}); err != nil {
		t.Fatalf("UpdateIssue %s: %v", k, err)
	}
}

func mustAttachLabel(t testing.TB, s *store.Store, iss *domain.Issue, projectKey, name string) {
	t.Helper()
	if err := s.AttachLabel(domain.IssueKey{Project: projectKey, Seq: iss.Seq}, name); err != nil {
		t.Fatalf("AttachLabel %s→%d: %v", name, iss.Seq, err)
	}
}

func TestSearch_TitleScoresHigherThanDescription(t *testing.T) {
	s := newTestStore(t)
	mustCreateProject(t, s, "AAA", "A")
	titleHit := mustCreateIssue(t, s, "AAA", "authentication system")
	_ = mustCreateIssue(t, s, "AAA", "unrelated work")
	// Put the query token only in the description of the second issue.
	mustUpdateDescription(t, s, "AAA", 2, "this body mentions authentication once")

	got, err := search.Search(context.Background(), s, search.Options{Query: "auth"})
	if err != nil {
		t.Fatalf("Search: %v", err)
	}
	if len(got) < 2 {
		t.Fatalf("want at least 2 matches, got %d (%+v)", len(got), got)
	}
	if got[0].Issue.ID != titleHit.ID {
		t.Fatalf("title match should rank first; got ID=%d want ID=%d", got[0].Issue.ID, titleHit.ID)
	}
	if got[0].Score <= got[1].Score {
		t.Fatalf("first match score %d should beat second %d", got[0].Score, got[1].Score)
	}
	if got[0].FieldScores["title"] == 0 {
		t.Fatalf("expected non-zero title field score; got %+v", got[0].FieldScores)
	}
	if len(got) < 2 {
		t.Fatalf("expected at least 2 matches (title + description), got %d", len(got))
	}
	if got[1].Issue.ID == titleHit.ID {
		t.Fatalf("description hit should be at index 1, found duplicate title hit")
	}
	if got[1].FieldScores["description"] == 0 {
		t.Fatalf("expected description match at rank 2; got FieldScores=%v", got[1].FieldScores)
	}
}

func TestSearch_DescriptionOnlyMatchStillSurfaces(t *testing.T) {
	s := newTestStore(t)
	mustCreateProject(t, s, "AAA", "A")
	iss := mustCreateIssue(t, s, "AAA", "unrelated title")
	mustUpdateDescription(t, s, "AAA", 1,
		"this is a really long description body that mentions authentication "+
			"somewhere in the middle and continues for many more characters here")

	got, err := search.Search(context.Background(), s, search.Options{Query: "auth"})
	if err != nil {
		t.Fatalf("Search: %v", err)
	}
	if len(got) != 1 || got[0].Issue.ID != iss.ID {
		t.Fatalf("description-only match should surface; got %d matches", len(got))
	}
	if got[0].BestField != "description" {
		t.Fatalf("BestField = %q, want %q", got[0].BestField, "description")
	}
	if len(got[0].Positions) == 0 {
		t.Fatalf("Positions should be populated even for negative-score description hits")
	}
}

func TestSearch_KeyFieldMatches(t *testing.T) {
	s := newTestStore(t)
	mustCreateProject(t, s, "AAA", "A")
	_ = mustCreateIssue(t, s, "AAA", "first")
	_ = mustCreateIssue(t, s, "AAA", "second")
	_ = mustCreateIssue(t, s, "AAA", "third") // becomes AAA-3

	got, err := search.Search(context.Background(), s, search.Options{Query: "AAA-3"})
	if err != nil {
		t.Fatalf("Search: %v", err)
	}
	if len(got) == 0 {
		t.Fatalf("expected at least one match for key 'AAA-3'")
	}
	if got[0].Issue.Seq != 3 {
		t.Fatalf("AAA-3 key match should rank first; got Seq=%d", got[0].Issue.Seq)
	}
	if got[0].ProjectKey != "AAA" {
		t.Fatalf("ProjectKey not populated; got %q", got[0].ProjectKey)
	}
	if got[0].FieldScores["key"] == 0 {
		t.Fatalf("expected non-zero key field score; got %+v", got[0].FieldScores)
	}
}

func TestSearch_LabelMatches(t *testing.T) {
	s := newTestStore(t)
	mustCreateProject(t, s, "AAA", "A")
	labeled := mustCreateIssue(t, s, "AAA", "totally unrelated")
	_ = mustCreateIssue(t, s, "AAA", "decoy work")
	mustAttachLabel(t, s, labeled, "AAA", "fuzzysearch")

	got, err := search.Search(context.Background(), s, search.Options{Query: "fuzzy"})
	if err != nil {
		t.Fatalf("Search: %v", err)
	}
	if len(got) == 0 {
		t.Fatalf("expected label match to surface")
	}
	if got[0].Issue.ID != labeled.ID {
		t.Fatalf("label match should rank first; got ID=%d want ID=%d", got[0].Issue.ID, labeled.ID)
	}
	if got[0].FieldScores["labels"] == 0 {
		t.Fatalf("expected non-zero labels field score; got %+v", got[0].FieldScores)
	}
	if len(got[0].Labels) != 1 || got[0].Labels[0] != "fuzzysearch" {
		t.Fatalf("Labels not populated; got %v", got[0].Labels)
	}
}

func TestSearch_FilterByLabel(t *testing.T) {
	s := newTestStore(t)
	mustCreateProject(t, s, "AAA", "A")
	keep := mustCreateIssue(t, s, "AAA", "auth keep target")
	_ = mustCreateIssue(t, s, "AAA", "auth decoy target")
	mustAttachLabel(t, s, keep, "AAA", "keep")

	got, err := search.Search(context.Background(), s, search.Options{
		Query:  "auth",
		Labels: []string{"keep"},
	})
	if err != nil {
		t.Fatalf("Search: %v", err)
	}
	if len(got) != 1 {
		t.Fatalf("label filter should narrow to 1 match; got %d (%+v)", len(got), got)
	}
	if got[0].Issue.ID != keep.ID {
		t.Fatalf("filter should return labeled issue; got ID=%d want ID=%d", got[0].Issue.ID, keep.ID)
	}
}

func TestSearch_EmptyQueryReturnsAllByRecency(t *testing.T) {
	s := newTestStore(t)
	mustCreateProject(t, s, "AAA", "A")
	_ = mustCreateIssue(t, s, "AAA", "older")
	time.Sleep(10 * time.Millisecond)
	newer := mustCreateIssue(t, s, "AAA", "newer")

	got, err := search.Search(context.Background(), s, search.Options{})
	if err != nil {
		t.Fatalf("Search: %v", err)
	}
	if len(got) < 2 {
		t.Fatalf("empty-query path should return all issues; got %d", len(got))
	}
	if got[0].Issue.ID != newer.ID {
		t.Fatalf("newest issue should rank first by UpdatedAt; got ID=%d want ID=%d", got[0].Issue.ID, newer.ID)
	}
	if got[0].Score != 0 {
		t.Fatalf("empty-query Score should be 0; got %d", got[0].Score)
	}
	if got[0].BestField != "" {
		t.Fatalf("empty-query BestField should be empty; got %q", got[0].BestField)
	}
}

func TestSearch_ArchivedExcludedByDefault(t *testing.T) {
	s := newTestStore(t)
	mustCreateProject(t, s, "AAA", "A")
	iss := mustCreateIssue(t, s, "AAA", "archived auth target")
	if err := s.SetIssueArchived(domain.IssueKey{Project: "AAA", Seq: iss.Seq}, true); err != nil {
		t.Fatalf("SetIssueArchived: %v", err)
	}

	got, err := search.Search(context.Background(), s, search.Options{Query: "auth"})
	if err != nil {
		t.Fatalf("Search default: %v", err)
	}
	if len(got) != 0 {
		t.Fatalf("archived issue should be excluded by default; got %d matches", len(got))
	}

	got, err = search.Search(context.Background(), s, search.Options{Query: "auth", IncludeArchived: true})
	if err != nil {
		t.Fatalf("Search IncludeArchived: %v", err)
	}
	if len(got) != 1 {
		t.Fatalf("IncludeArchived should surface archived match; got %d", len(got))
	}
}

func TestSearch_LimitTruncates(t *testing.T) {
	s := newTestStore(t)
	mustCreateProject(t, s, "AAA", "A")
	for i := 0; i < 10; i++ {
		_ = mustCreateIssue(t, s, "AAA", fmt.Sprintf("auth %d", i))
	}

	got, err := search.Search(context.Background(), s, search.Options{Query: "auth", Limit: 3})
	if err != nil {
		t.Fatalf("Search: %v", err)
	}
	if len(got) != 3 {
		t.Fatalf("Limit=3 should truncate; got %d matches", len(got))
	}
}
