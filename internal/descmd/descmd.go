// Package descmd parses and mutates the cliban issue/milestone description
// markdown contract. The contract is documented in the cliban README under
// "Description contract". All functions are pure: input string in, output
// string + error out. The store layer wraps these in SQL transactions so
// mutations are atomic.
package descmd

import (
	"fmt"
	"strings"
	"time"
)

// FindSection locates a top-level H2 section by its exact anchor text
// (the part after "## "). It returns the [start, end) byte offsets of the
// section's *content* — i.e. everything after the heading line up to (but
// not including) the next H2 heading or end of string.
//
// Matching rules:
//   - Anchor match is case-sensitive and exact (no leading/trailing spaces).
//   - The heading must appear at the start of a line.
//   - Content includes the leading newline after the heading and the
//     trailing newlines up to the next ## heading.
func FindSection(desc, anchor string) (start, end int, found bool) {
	if anchor == "" {
		return 0, 0, false
	}
	needle := "## " + anchor
	lines := strings.SplitAfter(desc, "\n")
	offset := 0
	sectionContentStart := -1
	for _, line := range lines {
		lineLen := len(line)
		trimmed := strings.TrimRight(line, "\r\n")
		if sectionContentStart < 0 {
			if trimmed == needle {
				sectionContentStart = offset + lineLen
			}
		} else if strings.HasPrefix(trimmed, "## ") {
			return sectionContentStart, offset, true
		}
		offset += lineLen
	}
	if sectionContentStart < 0 {
		return 0, 0, false
	}
	return sectionContentStart, len(desc), true
}

// errf is a small helper for constructing descmd errors with structured prefixes.
func errf(format string, args ...any) error {
	return fmt.Errorf("descmd: "+format, args...)
}

// activityLogTimeLayout is the timestamp format used for ## Activity Log
// entries: RFC-3339 with minute precision, UTC, "Z" suffix.
const activityLogTimeLayout = "2006-01-02T15:04Z"

// FindTask locates the N-th task within a plan-section body. Tasks are
// identified by an H3 heading of the form "### Task <N>:" at the start of a
// line. The matching is exact: a search for Task 1 will not match
// "### Task 10:" (the trailing colon in the prefix prevents that).
// Returns the [start, end) byte offsets of the task's body — content
// AFTER the heading line, up to (but excluding) the next "### " heading
// or end of input.
func FindTask(planBody string, n int) (start, end int, found bool) {
	prefix := fmt.Sprintf("### Task %d:", n)
	lines := strings.SplitAfter(planBody, "\n")
	offset := 0
	taskBodyStart := -1
	for _, line := range lines {
		lineLen := len(line)
		trimmed := strings.TrimRight(line, "\r\n")
		if taskBodyStart < 0 {
			if strings.HasPrefix(trimmed, prefix) {
				taskBodyStart = offset + lineLen
			}
		} else if strings.HasPrefix(trimmed, "### ") {
			return taskBodyStart, offset, true
		}
		offset += lineLen
	}
	if taskBodyStart < 0 {
		return 0, 0, false
	}
	return taskBodyStart, len(planBody), true
}

// Step describes one bite-sized step line in a Task body.
type Step struct {
	Index     int    // 1-based step index within the task
	Checked   bool   // current checkbox state
	LineStart int    // byte offset of the line start within the task body
	LineEnd   int    // byte offset just past the trailing newline (or len(task) if last)
	Raw       string // the full line content including the trailing newline (and \r if input uses CRLF) — preserved verbatim for round-trip mutations
}

// FindStep locates the M-th step line in a task body. Steps are top-level
// GFM checkbox list items: lines beginning with "- [ ] " or "- [x] " at
// column zero. Indented child bullets are ignored.
func FindStep(taskBody string, m int) (Step, bool) {
	lines := strings.SplitAfter(taskBody, "\n")
	offset := 0
	count := 0
	for _, line := range lines {
		lineLen := len(line)
		if strings.HasPrefix(line, "- [ ] ") || strings.HasPrefix(line, "- [x] ") {
			count++
			if count == m {
				return Step{
					Index:     m,
					Checked:   strings.HasPrefix(line, "- [x] "),
					LineStart: offset,
					LineEnd:   offset + lineLen,
					Raw:       line,
				}, true
			}
		}
		offset += lineLen
	}
	return Step{}, false
}

// TickStep flips the M-th step of task N in the description from
// "- [ ] ..." to "- [x] ...". Returns the rewritten description.
// Returns a non-nil error if the ## Plan section is missing, the task is
// missing, the step is missing, or the step is already checked.
func TickStep(desc string, taskN, stepM int) (string, error) {
	planStart, planEnd, ok := FindSection(desc, "Plan")
	if !ok {
		return "", errf("no ## Plan section")
	}
	planBody := desc[planStart:planEnd]
	taskStart, taskEnd, ok := FindTask(planBody, taskN)
	if !ok {
		return "", errf("no Task %d in ## Plan", taskN)
	}
	taskBody := planBody[taskStart:taskEnd]
	step, ok := FindStep(taskBody, stepM)
	if !ok {
		return "", errf("no Step %d in Task %d", stepM, taskN)
	}
	if step.Checked {
		return "", errf("Step %d of Task %d already checked", stepM, taskN)
	}
	// Absolute offset of the step line inside the original desc.
	abs := planStart + taskStart + step.LineStart
	// The step line is guaranteed to start with "- [ ] ".
	newLine := "- [x] " + step.Raw[len("- [ ] "):]
	return desc[:abs] + newLine + desc[abs+len(step.Raw):], nil
}

// AppendActivityLog appends a chronological entry to the "## Activity Log"
// section. The entry is formatted as "- <UTC-ts> — <msg>" with the timestamp
// rendered as "2006-01-02T15:04Z" (RFC-3339 minutes, UTC). If the section
// does not exist, one is created at the end of the description.
//
// Normalization: the resulting section always ends with exactly one trailing
// newline when Activity Log is the last section in desc, or two trailing
// newlines when followed by another section (the second newline being the
// inter-section blank line preserved per markdown convention).
func AppendActivityLog(desc, msg string, ts time.Time) string {
	stamp := ts.UTC().Format(activityLogTimeLayout)
	entry := fmt.Sprintf("- %s — %s\n", stamp, msg)
	start, end, ok := FindSection(desc, "Activity Log")
	if !ok {
		sep := ""
		if desc != "" {
			sep = "\n"
			if !strings.HasSuffix(desc, "\n") {
				sep = "\n\n"
			} else if !strings.HasSuffix(desc, "\n\n") {
				sep = "\n"
			}
		}
		return desc + sep + "## Activity Log\n\n" + entry
	}
	// Insert the entry at the end of the section body. The body may end with
	// one or two trailing newlines depending on whether Activity Log is the
	// last section in desc (one newline) or is followed by another section
	// (two newlines — the second one being the inter-section blank line).
	// Strip trailing newlines, append the new entry (which ends with \n),
	// then restore the blank-line separator if not the last section.
	body := desc[start:end]
	trimmed := strings.TrimRight(body, "\n")
	rebuilt := trimmed + "\n" + entry
	if end < len(desc) {
		// Not the last section: restore the blank-line separator before the
		// following ## heading. The entry's own trailing \n closes the entry
		// line; the extra \n is the blank line.
		rebuilt += "\n"
	}
	return desc[:start] + rebuilt + desc[end:]
}

// RewriteStepLine replaces the M-th step line in task N with the provided
// newLine. newLine must end with a single newline character; otherwise an
// error is returned. The caller is responsible for ensuring newLine remains
// a valid step (starts with "- [ ] " or "- [x] ") — the function performs
// no syntax validation on newLine's content. Typical use: appending the
// "→ KEY" promotion suffix.
func RewriteStepLine(desc string, taskN, stepM int, newLine string) (string, error) {
	if !strings.HasSuffix(newLine, "\n") {
		return "", errf("newLine must end with newline")
	}
	planStart, planEnd, ok := FindSection(desc, "Plan")
	if !ok {
		return "", errf("no ## Plan section")
	}
	planBody := desc[planStart:planEnd]
	taskStart, taskEnd, ok := FindTask(planBody, taskN)
	if !ok {
		return "", errf("no Task %d in ## Plan", taskN)
	}
	taskBody := planBody[taskStart:taskEnd]
	step, ok := FindStep(taskBody, stepM)
	if !ok {
		return "", errf("no Step %d in Task %d", stepM, taskN)
	}
	abs := planStart + taskStart + step.LineStart
	return desc[:abs] + newLine + desc[abs+len(step.Raw):], nil
}
