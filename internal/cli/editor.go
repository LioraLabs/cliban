package cli

import (
	"github.com/alex/cliban/internal/issuebuf"
)

// ResolveEditor delegates to issuebuf.ResolveEditor.
func ResolveEditor() string {
	return issuebuf.ResolveEditor()
}

// EditorDisabled delegates to issuebuf.EditorDisabled.
func EditorDisabled(noEditorFlag bool) bool {
	return issuebuf.EditorDisabled(noEditorFlag)
}

// RequireTTY delegates to issuebuf.RequireTTY.
func RequireTTY() error {
	return issuebuf.RequireTTY()
}

// RunEditor delegates to issuebuf.RunEditor.
func RunEditor(path string) error {
	return issuebuf.RunEditor(path)
}

// WriteTempBuffer delegates to issuebuf.WriteTempBuffer.
func WriteTempBuffer(prefix, contents string) (string, error) {
	return issuebuf.WriteTempBuffer(prefix, contents)
}
