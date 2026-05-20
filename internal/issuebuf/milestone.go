package issuebuf

import (
	"bytes"
	"fmt"
	"strings"

	"gopkg.in/yaml.v3"
)

type MilestoneBuffer struct {
	Header      string `yaml:"-"`
	Name        string `yaml:"name"`
	Target      string `yaml:"target"`
	Status      string `yaml:"status"`
	Description string `yaml:"-"`
}

func (b MilestoneBuffer) Serialize() string {
	var buf bytes.Buffer
	if b.Header != "" {
		buf.WriteString(b.Header)
		if !strings.HasSuffix(b.Header, "\n") {
			buf.WriteString("\n")
		}
	}
	fmt.Fprintf(&buf, "---\nname:   %s\ntarget: %s\nstatus: %s\n---\n",
		b.Name, b.Target, b.Status)
	buf.WriteString(b.Description)
	if !strings.HasSuffix(b.Description, "\n") {
		buf.WriteString("\n")
	}
	return buf.String()
}

func ParseMilestoneBuffer(src string) (MilestoneBuffer, error) {
	if strings.TrimSpace(src) == "" {
		return MilestoneBuffer{}, fmt.Errorf("buffer is empty")
	}
	frontMatter, body, err := splitFrontmatter(src)
	if err != nil {
		return MilestoneBuffer{}, err
	}
	var b MilestoneBuffer
	if err := yaml.Unmarshal([]byte(frontMatter), &b); err != nil {
		return MilestoneBuffer{}, fmt.Errorf("frontmatter parse: %w", err)
	}
	b.Description = strings.TrimSpace(body) + "\n"
	if strings.TrimSpace(b.Description) == "" {
		b.Description = ""
	}
	if b.Name == "" {
		return MilestoneBuffer{}, fmt.Errorf("name is required")
	}
	if b.Status != "" {
		switch b.Status {
		case "open", "completed", "cancelled":
		default:
			return MilestoneBuffer{}, fmt.Errorf("invalid status %q (valid: open, completed, cancelled)", b.Status)
		}
	}
	return b, nil
}

// splitFrontmatter is shared with ParseIssueBuffer; returns the YAML frontmatter
// section and the body after the closing '---' line.
func splitFrontmatter(src string) (frontMatter, body string, err error) {
	lines := strings.Split(src, "\n")
	firstDelim := -1
	for i, l := range lines {
		if strings.TrimSpace(l) == "---" {
			firstDelim = i
			break
		}
	}
	if firstDelim < 0 {
		return "", "", fmt.Errorf("missing opening '---' line")
	}
	rest := lines[firstDelim+1:]
	secondDelim := -1
	for i, l := range rest {
		if strings.TrimSpace(l) == "---" {
			secondDelim = i
			break
		}
	}
	if secondDelim < 0 {
		return "", "", fmt.Errorf("missing closing '---' line")
	}
	return strings.Join(rest[:secondDelim], "\n"), strings.Join(rest[secondDelim+1:], "\n"), nil
}
