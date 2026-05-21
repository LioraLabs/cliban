package search_test

import (
	"context"
	"fmt"
	"testing"

	"github.com/alex/cliban/internal/search"
)

func BenchmarkSearch_5kIssues(b *testing.B) {
	s := newTestStore(b)
	mustCreateProject(b, s, "BIG", "Big")
	for i := 0; i < 5000; i++ {
		mustCreateIssue(b, s, "BIG", fmt.Sprintf("synthetic title %d about authentication and tokens", i))
	}
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_, err := search.Search(context.Background(), s, search.Options{Query: "auth"})
		if err != nil {
			b.Fatal(err)
		}
	}
}
