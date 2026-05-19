package issuebuf

import (
	"fmt"
	"os"
	"os/exec"

	"golang.org/x/term"
)

func ResolveEditor() string {
	if v := os.Getenv("VISUAL"); v != "" {
		return v
	}
	if v := os.Getenv("EDITOR"); v != "" {
		return v
	}
	return "vi"
}

func EditorDisabled(noEditorFlag bool) bool {
	if noEditorFlag {
		return true
	}
	if os.Getenv("CLIBAN_NO_EDITOR") != "" {
		return true
	}
	return false
}

func RequireTTY() error {
	if os.Getenv("CLIBAN_FORCE_TTY") != "" {
		return nil
	}
	if !term.IsTerminal(int(os.Stdin.Fd())) {
		return fmt.Errorf("stdin is not a TTY; cannot open editor (pass --title/--description or --no-editor)")
	}
	return nil
}

// RunEditor execs $EDITOR <path>, sharing the current TTY.
// EDITOR may include args (e.g. "code --wait" or "sh -c 'echo >> $1' --").
func RunEditor(path string) error {
	editor := ResolveEditor()
	cmd := exec.Command("sh", "-c", fmt.Sprintf("%s %q", editor, path))
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	return cmd.Run()
}

// WriteTempBuffer writes contents to a deterministically-named temp file and returns its path.
func WriteTempBuffer(prefix, contents string) (string, error) {
	f, err := os.CreateTemp("", prefix+"-*.md")
	if err != nil {
		return "", err
	}
	defer f.Close()
	if _, err := f.WriteString(contents); err != nil {
		return "", err
	}
	return f.Name(), nil
}
