package descmd

import (
	"strings"
	"testing"
	"time"
)

func TestFindSection_Found(t *testing.T) {
	desc := "## Spec\n\nhello\n\n## Plan\n\n### Task 1: foo\n"
	start, end, ok := FindSection(desc, "Spec")
	if !ok {
		t.Fatalf("expected to find ## Spec section")
	}
	got := desc[start:end]
	want := "\nhello\n\n"
	if got != want {
		t.Fatalf("content mismatch:\n got=%q\nwant=%q", got, want)
	}
}

func TestFindSection_NotFound(t *testing.T) {
	desc := "no sections here"
	if _, _, ok := FindSection(desc, "Spec"); ok {
		t.Fatalf("expected not found")
	}
}

func TestFindSection_LastSection(t *testing.T) {
	desc := "## Spec\n\nhello world"
	start, end, ok := FindSection(desc, "Spec")
	if !ok {
		t.Fatalf("expected found")
	}
	got := desc[start:end]
	want := "\nhello world"
	if got != want {
		t.Fatalf("content mismatch:\n got=%q\nwant=%q", got, want)
	}
}

func TestFindSection_NoFalseMatchOnPrefix(t *testing.T) {
	desc := "## Specification\n\nnot spec\n"
	if _, _, ok := FindSection(desc, "Spec"); ok {
		t.Fatalf("anchor %q must not match %q (exact match required)", "Spec", "## Specification")
	}
}

func TestFindSection_NoFalseMatchOnH3(t *testing.T) {
	desc := "### Spec\n\nnot a top-level section\n"
	if _, _, ok := FindSection(desc, "Spec"); ok {
		t.Fatalf("must not match H3 ### Spec as a section")
	}
}

func TestFindSection_FindsNonFirstSection(t *testing.T) {
	desc := "## Spec\n\nspec body\n\n## Plan\n\nplan body\n\n## Activity Log\n\nlog body\n"
	start, end, ok := FindSection(desc, "Plan")
	if !ok {
		t.Fatalf("expected to find ## Plan section")
	}
	got := desc[start:end]
	want := "\nplan body\n\n"
	if got != want {
		t.Fatalf("Plan section content mismatch:\n got=%q\nwant=%q", got, want)
	}
}

func TestFindSection_ByteOffsets_AnchorAtStart(t *testing.T) {
	desc := "## Spec\nbody\n"
	start, end, ok := FindSection(desc, "Spec")
	if !ok {
		t.Fatalf("expected found")
	}
	const wantStart = len("## Spec\n") // 8
	if start != wantStart {
		t.Fatalf("start offset = %d, want %d", start, wantStart)
	}
	if end != len(desc) {
		t.Fatalf("end offset = %d, want %d (len(desc))", end, len(desc))
	}
}

func TestFindTask_Found(t *testing.T) {
	plan := "\n### Task 1: foo\n\nbody1\n\n### Task 2: bar\n\nbody2\n"
	start, end, ok := FindTask(plan, 1)
	if !ok {
		t.Fatalf("expected to find Task 1")
	}
	got := plan[start:end]
	want := "\nbody1\n\n"
	if got != want {
		t.Fatalf("content mismatch:\n got=%q\nwant=%q", got, want)
	}
}

func TestFindTask_NotFound(t *testing.T) {
	plan := "\n### Task 1: foo\n"
	if _, _, ok := FindTask(plan, 2); ok {
		t.Fatalf("expected Task 2 not found")
	}
}

func TestFindTask_LastTask(t *testing.T) {
	plan := "\n### Task 1: foo\n\nbody1\n"
	start, end, ok := FindTask(plan, 1)
	if !ok {
		t.Fatalf("expected found")
	}
	if plan[start:end] != "\nbody1\n" {
		t.Fatalf("got=%q want=%q", plan[start:end], "\nbody1\n")
	}
}

func TestFindStep_Found(t *testing.T) {
	task := "\n- [ ] **Step 1: foo**\n- [ ] **Step 2: bar**\n- [x] **Step 3: baz**\n"
	step, ok := FindStep(task, 2)
	if !ok {
		t.Fatalf("expected to find step 2")
	}
	if step.Checked {
		t.Fatalf("step 2 should be unchecked")
	}
	if got := task[step.LineStart:step.LineEnd]; got != "- [ ] **Step 2: bar**\n" {
		t.Fatalf("line content mismatch: %q", got)
	}
}

func TestFindStep_AlreadyChecked(t *testing.T) {
	task := "\n- [ ] **Step 1: foo**\n- [x] **Step 2: bar**\n"
	step, ok := FindStep(task, 2)
	if !ok {
		t.Fatalf("expected found")
	}
	if !step.Checked {
		t.Fatalf("step 2 should be checked")
	}
}

func TestFindStep_OutOfRange(t *testing.T) {
	task := "\n- [ ] **Step 1: foo**\n"
	if _, ok := FindStep(task, 2); ok {
		t.Fatalf("step 2 should not exist")
	}
}

func TestFindStep_IndentedChildIgnored(t *testing.T) {
	task := "\n- [ ] **Step 1: foo**\n  - some nested bullet\n- [ ] **Step 2: bar**\n"
	step, ok := FindStep(task, 2)
	if !ok {
		t.Fatalf("expected step 2 found")
	}
	if got := task[step.LineStart:step.LineEnd]; got != "- [ ] **Step 2: bar**\n" {
		t.Fatalf("got=%q want=%q", got, "- [ ] **Step 2: bar**\n")
	}
}

func TestFindTask_FindsNonFirstTask(t *testing.T) {
	plan := "\n### Task 1: foo\n\nbody1\n\n### Task 2: bar\n\nbody2\n\n### Task 3: baz\n\nbody3\n"
	start, end, ok := FindTask(plan, 2)
	if !ok {
		t.Fatalf("expected to find Task 2")
	}
	got := plan[start:end]
	want := "\nbody2\n\n"
	if got != want {
		t.Fatalf("Task 2 content mismatch:\n got=%q\nwant=%q", got, want)
	}
}

func TestFindTask_TwoDigitNumber(t *testing.T) {
	plan := "\n### Task 1: foo\n\nbody1\n\n### Task 10: ten\n\nbody10\n"
	// Searching for Task 10 must not be misled by the "10" sharing the "1" prefix.
	start, end, ok := FindTask(plan, 10)
	if !ok {
		t.Fatalf("expected to find Task 10")
	}
	got := plan[start:end]
	want := "\nbody10\n"
	if got != want {
		t.Fatalf("Task 10 content mismatch:\n got=%q\nwant=%q", got, want)
	}
	// Sanity check: Task 1 still resolves correctly when Task 10 also exists.
	s1, e1, ok := FindTask(plan, 1)
	if !ok {
		t.Fatalf("expected to find Task 1")
	}
	if got := plan[s1:e1]; got != "\nbody1\n\n" {
		t.Fatalf("Task 1 content mismatch:\n got=%q", got)
	}
}

func TestTickStep_HappyPath(t *testing.T) {
	desc := "## Spec\n\nx\n\n## Plan\n\n### Task 1: foo\n\n- [ ] **Step 1: a**\n- [ ] **Step 2: b**\n"
	out, err := TickStep(desc, 1, 2)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	want := "## Spec\n\nx\n\n## Plan\n\n### Task 1: foo\n\n- [ ] **Step 1: a**\n- [x] **Step 2: b**\n"
	if out != want {
		t.Fatalf("output mismatch:\n got=%q\nwant=%q", out, want)
	}
}

func TestTickStep_AlreadyChecked(t *testing.T) {
	desc := "## Plan\n\n### Task 1: foo\n\n- [x] **Step 1: a**\n"
	_, err := TickStep(desc, 1, 1)
	if err == nil || !strings.Contains(err.Error(), "already checked") {
		t.Fatalf("expected already-checked error, got %v", err)
	}
}

func TestTickStep_NoPlanSection(t *testing.T) {
	desc := "## Spec\n\nonly spec here\n"
	_, err := TickStep(desc, 1, 1)
	if err == nil || !strings.Contains(err.Error(), "no ## Plan section") {
		t.Fatalf("expected no-plan error, got %v", err)
	}
}

func TestTickStep_NoTask(t *testing.T) {
	desc := "## Plan\n\n### Task 1: foo\n\n- [ ] **Step 1: a**\n"
	_, err := TickStep(desc, 5, 1)
	if err == nil || !strings.Contains(err.Error(), "no Task 5") {
		t.Fatalf("expected no-task error, got %v", err)
	}
}

func TestTickStep_NoStep(t *testing.T) {
	desc := "## Plan\n\n### Task 1: foo\n\n- [ ] **Step 1: a**\n"
	_, err := TickStep(desc, 1, 9)
	if err == nil || !strings.Contains(err.Error(), "no Step 9") {
		t.Fatalf("expected no-step error, got %v", err)
	}
}

func TestAppendActivityLog_ExistingSection(t *testing.T) {
	desc := "## Spec\n\nfoo\n\n## Activity Log\n\n- 2026-05-19T10:00Z — earlier\n"
	ts, _ := time.Parse(time.RFC3339, "2026-05-20T13:42:00Z")
	out := AppendActivityLog(desc, "promoted Step 3", ts)
	want := "## Spec\n\nfoo\n\n## Activity Log\n\n- 2026-05-19T10:00Z — earlier\n- 2026-05-20T13:42Z — promoted Step 3\n"
	if out != want {
		t.Fatalf("output mismatch:\n got=%q\nwant=%q", out, want)
	}
}

func TestAppendActivityLog_CreatesSectionWhenAbsent(t *testing.T) {
	desc := "## Spec\n\nfoo\n"
	ts, _ := time.Parse(time.RFC3339, "2026-05-20T13:42:00Z")
	out := AppendActivityLog(desc, "first entry", ts)
	want := "## Spec\n\nfoo\n\n## Activity Log\n\n- 2026-05-20T13:42Z — first entry\n"
	if out != want {
		t.Fatalf("output mismatch:\n got=%q\nwant=%q", out, want)
	}
}

func TestAppendActivityLog_EmptyDescription(t *testing.T) {
	ts, _ := time.Parse(time.RFC3339, "2026-05-20T13:42:00Z")
	out := AppendActivityLog("", "first entry", ts)
	want := "## Activity Log\n\n- 2026-05-20T13:42Z — first entry\n"
	if out != want {
		t.Fatalf("output mismatch:\n got=%q\nwant=%q", out, want)
	}
}

func TestRewriteStepLine_HappyPath(t *testing.T) {
	desc := "## Plan\n\n### Task 1: foo\n\n- [ ] **Step 1: a**\n- [ ] **Step 2: b**\n"
	out, err := RewriteStepLine(desc, 1, 2, "- [ ] **Step 2: b** → CLI-99\n")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	want := "## Plan\n\n### Task 1: foo\n\n- [ ] **Step 1: a**\n- [ ] **Step 2: b** → CLI-99\n"
	if out != want {
		t.Fatalf("output mismatch:\n got=%q\nwant=%q", out, want)
	}
}

func TestRewriteStepLine_MissingNewline(t *testing.T) {
	_, err := RewriteStepLine("## Plan\n\n### Task 1: foo\n\n- [ ] x\n", 1, 1, "no newline")
	if err == nil || !strings.Contains(err.Error(), "must end with newline") {
		t.Fatalf("expected newline-required error, got %v", err)
	}
}

func TestAppendActivityLog_PreservesSeparatorWhenFollowedBySection(t *testing.T) {
	// When ## Activity Log is followed by another H2 section, the inter-section
	// blank line MUST be preserved. Without this, sections collide.
	desc := "## Activity Log\n\n- earlier\n\n## Notes\n\nmore content\n"
	ts, _ := time.Parse(time.RFC3339, "2026-05-20T13:42:00Z")
	out := AppendActivityLog(desc, "new entry", ts)
	want := "## Activity Log\n\n- earlier\n- 2026-05-20T13:42Z — new entry\n\n## Notes\n\nmore content\n"
	if out != want {
		t.Fatalf("output mismatch:\n got=%q\nwant=%q", out, want)
	}
}

func TestAppendActivityLog_EmptySectionFollowedBySection(t *testing.T) {
	// Edge case: Activity Log section exists but has no entries yet, and is
	// followed by another section. The new entry must land cleanly between
	// the heading and the following section, with proper blank-line separators.
	desc := "## Activity Log\n\n## Notes\n\nmore\n"
	ts, _ := time.Parse(time.RFC3339, "2026-05-20T13:42:00Z")
	out := AppendActivityLog(desc, "first entry", ts)
	want := "## Activity Log\n\n- 2026-05-20T13:42Z — first entry\n\n## Notes\n\nmore\n"
	if out != want {
		t.Fatalf("output mismatch:\n got=%q\nwant=%q", out, want)
	}
}
