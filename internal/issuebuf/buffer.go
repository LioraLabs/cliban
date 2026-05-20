// Package issuebuf provides the IssueBuffer type used by both the CLI and TUI
// to serialize/deserialize issue editor files.
package issuebuf

import (
	"bytes"
	"fmt"
	"strings"

	"github.com/alex/cliban/internal/domain"
	"gopkg.in/yaml.v3"
)

type IssueBuffer struct {
	Header      string `yaml:"-"`
	Title       string `yaml:"title"`
	Status      string `yaml:"status"`
	Priority    string `yaml:"priority"`
	Milestone   string `yaml:"milestone"`
	Parent      string `yaml:"parent"`
	Description string `yaml:"-"`
}

// Serialize returns the on-disk buffer: header comments + YAML frontmatter + markdown body.
func (b IssueBuffer) Serialize() string {
	var buf bytes.Buffer
	if b.Header != "" {
		buf.WriteString(b.Header)
		if !strings.HasSuffix(b.Header, "\n") {
			buf.WriteString("\n")
		}
	}
	fmt.Fprintf(&buf, "---\ntitle:     %s\nstatus:    %s\npriority:  %s\nmilestone: %s\nparent:    %s\n---\n",
		b.Title, b.Status, b.Priority, b.Milestone, b.Parent)
	buf.WriteString(b.Description)
	if !strings.HasSuffix(b.Description, "\n") {
		buf.WriteString("\n")
	}
	return buf.String()
}

func ParseIssueBuffer(src string) (IssueBuffer, error) {
	if strings.TrimSpace(src) == "" {
		return IssueBuffer{}, fmt.Errorf("buffer is empty")
	}
	frontMatter, body, err := splitFrontmatter(src)
	if err != nil {
		return IssueBuffer{}, err
	}

	var b IssueBuffer
	if err := yaml.Unmarshal([]byte(frontMatter), &b); err != nil {
		return IssueBuffer{}, fmt.Errorf("frontmatter parse: %w", err)
	}
	b.Description = strings.TrimSpace(body) + "\n"
	if strings.TrimSpace(b.Description) == "" {
		b.Description = ""
	}
	if b.Title == "" {
		return IssueBuffer{}, fmt.Errorf("title is required")
	}
	if b.Status != "" {
		if _, err := domain.ParseStatus(b.Status); err != nil {
			return IssueBuffer{}, err
		}
	}
	if b.Priority != "" {
		if _, err := domain.ParsePriority(b.Priority); err != nil {
			return IssueBuffer{}, err
		}
	}
	return b, nil
}
