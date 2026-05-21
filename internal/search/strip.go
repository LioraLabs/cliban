package search

import "regexp"

var (
	reFence   = regexp.MustCompile("(?s)```.*?```")
	reHeading = regexp.MustCompile(`(?m)^#+\s*`)
	reLink    = regexp.MustCompile(`\[([^\]]+)\]\([^)]+\)`)
)

const maxDescBytes = 4096

// stripDescription performs a conservative regex pass over markdown-ish input
// to make it suitable for fuzzy matching. It is NOT a real markdown parser:
// it collapses fenced code blocks, drops heading markers, unwraps links to
// their visible text, and caps the result at maxDescBytes.
func stripDescription(s string) string {
	s = reFence.ReplaceAllString(s, " ")
	s = reHeading.ReplaceAllString(s, "")
	s = reLink.ReplaceAllString(s, "$1")
	if len(s) > maxDescBytes {
		s = s[:maxDescBytes]
	}
	return s
}
