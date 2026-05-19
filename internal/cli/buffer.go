package cli

import (
	"github.com/alex/cliban/internal/issuebuf"
)

// IssueBuffer is re-exported from issuebuf for backwards compatibility.
type IssueBuffer = issuebuf.IssueBuffer

// ParseIssueBuffer delegates to issuebuf.ParseIssueBuffer.
func ParseIssueBuffer(src string) (IssueBuffer, error) {
	return issuebuf.ParseIssueBuffer(src)
}
