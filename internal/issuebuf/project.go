package issuebuf

import (
	"bytes"
	"fmt"
	"strings"

	"gopkg.in/yaml.v3"
)

type ProjectBuffer struct {
	Header      string `yaml:"-"`
	Name        string `yaml:"name"`
	Description string `yaml:"-"`
}

func (b ProjectBuffer) Serialize() string {
	var buf bytes.Buffer
	if b.Header != "" {
		buf.WriteString(b.Header)
		if !strings.HasSuffix(b.Header, "\n") {
			buf.WriteString("\n")
		}
	}
	fmt.Fprintf(&buf, "---\nname: %s\n---\n", b.Name)
	buf.WriteString(b.Description)
	if !strings.HasSuffix(b.Description, "\n") {
		buf.WriteString("\n")
	}
	return buf.String()
}

func ParseProjectBuffer(src string) (ProjectBuffer, error) {
	if strings.TrimSpace(src) == "" {
		return ProjectBuffer{}, fmt.Errorf("buffer is empty")
	}
	frontMatter, body, err := splitFrontmatter(src)
	if err != nil {
		return ProjectBuffer{}, err
	}
	var b ProjectBuffer
	if err := yaml.Unmarshal([]byte(frontMatter), &b); err != nil {
		return ProjectBuffer{}, fmt.Errorf("frontmatter parse: %w", err)
	}
	b.Description = strings.TrimSpace(body) + "\n"
	if strings.TrimSpace(b.Description) == "" {
		b.Description = ""
	}
	if b.Name == "" {
		return ProjectBuffer{}, fmt.Errorf("name is required")
	}
	return b, nil
}
