//! `cliban issue lint` — validate the description contract before it bites.
//!
//! The contract (`## Spec` / `## Plan` / `## Activity Log`, `### Task N:`
//! headings, GFM checkbox steps) is what `tick`, `promote`, and every reader
//! of `--section` rely on. An agent that hand-writes a plan finds out it's
//! malformed only when `tick` fails mid-execution; lint moves that discovery
//! to write time. Pure functions over the description text — the store is not
//! consulted.

use crate::descmd::{find_section, find_step, find_task};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Breaks a tool (`tick` will refuse, a section won't parse).
    Error,
    /// Parses, but not as the contract intends (a skipped task number, an
    /// activity line no reader will surface).
    Warning,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub severity: Severity,
    pub message: String,
}

fn err(findings: &mut Vec<Finding>, message: String) {
    findings.push(Finding {
        severity: Severity::Error,
        message,
    });
}

fn warn(findings: &mut Vec<Finding>, message: String) {
    findings.push(Finding {
        severity: Severity::Warning,
        message,
    });
}

/// Lint one issue description. Empty result = clean.
pub fn lint_description(desc: &str) -> Vec<Finding> {
    let mut findings = Vec::new();

    if !desc.trim().is_empty() && !find_section(desc, "Spec").2 {
        warn(&mut findings, "no ## Spec section".to_string());
    }

    let (plan_start, plan_end, has_plan) = find_section(desc, "Plan");
    if has_plan {
        lint_plan(&desc[plan_start..plan_end], &mut findings);
    }

    if find_section(desc, "Activity Log").2 {
        lint_activity(desc, &mut findings);
    }

    findings
}

fn lint_plan(plan: &str, findings: &mut Vec<Finding>) {
    // Every H3 in the plan must be a task heading; collect the numbers, and
    // note where the first one starts — a checkbox before it lints clean
    // structurally but `tick` can never address it.
    //
    // Headings and list items come from the markdown grammar (the same source
    // `tick` reads), so a `### Task 2:` or a `- [X]` quoted inside a fenced
    // code block is content, not structure, and no longer reported.
    let mut numbers: Vec<i32> = Vec::new();
    let mut first_task_at: Option<usize> = None;
    let mut unreachable_steps = 0usize;
    for h in cliban_core::sections::h3_headings(plan) {
        match cliban_core::sections::task_number(&h.text) {
            Some(n) => {
                numbers.push(n);
                first_task_at.get_or_insert(h.range.start);
            }
            None => err(
                findings,
                format!(
                    "plan heading {:?} is not \"### Task N: <title>\"",
                    plan[h.range.clone()].trim_end()
                ),
            ),
        }
    }

    for item in cliban_core::sections::list_items(plan) {
        let src = &plan[item.range.clone()];
        let first_line = src.split_once('\n').map_or(src, |(l, _)| l).trim_end();
        if !first_line.starts_with("- [") {
            continue;
        }
        let well_formed = first_line.starts_with("- [ ] ") || first_line.starts_with("- [x] ");
        // Checkboxes tick/find_step will not recognize. At the top level that's
        // a malformed step (error); nested it's a child bullet — legitimate per
        // the contract, but silently untickable, so worth a heads-up.
        if item.depth > 0 {
            warn(
                findings,
                format!(
                    "indented checkbox {first_line:?} is a child bullet — tick cannot reach it"
                ),
            );
        } else if !well_formed {
            err(
                findings,
                format!("malformed checkbox {first_line:?} (want \"- [ ] \" or \"- [x] \")"),
            );
        } else if first_task_at.is_none_or(|at| item.range.start < at) {
            unreachable_steps += 1;
        }
    }

    if unreachable_steps > 0 {
        warn(
            findings,
            format!(
                "{unreachable_steps} step(s) outside any \"### Task N:\" heading — \
                 tick cannot reach them"
            ),
        );
    }

    for (i, n) in numbers.iter().enumerate() {
        let expected = (i + 1) as i32;
        if *n != expected {
            warn(
                findings,
                format!(
                    "plan tasks are numbered {numbers:?}; expected 1..{}",
                    numbers.len()
                ),
            );
            break;
        }
    }

    for n in &numbers {
        let (task_start, task_end, ok) = find_task(plan, *n);
        if !ok {
            continue; // duplicate number: find_task sees the first; the sequence check flagged it
        }
        if find_step(&plan[task_start..task_end], 1).is_none() {
            warn(findings, format!("Task {n} has no steps"));
        }
    }
}

/// `desc` rather than the section slice: the entry parser locates
/// `## Activity Log` itself, and routing lint through the very function the
/// reader uses is what stops the two from disagreeing about what an entry is.
fn lint_activity(desc: &str, findings: &mut Vec<Finding>) {
    for entry in crate::descmd::activity_entries(desc) {
        if entry.parsed.is_none() {
            let head = &entry.head;
            warn(
                findings,
                format!("activity entry \"- {head}\" does not parse as \"- <ts> — <msg>\""),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn errors(desc: &str) -> usize {
        lint_description(desc)
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .count()
    }

    #[test]
    fn a_conforming_description_is_clean() {
        let d = "## Spec\n\ns\n\n## Plan\n\n### Task 1: a\n\n- [ ] **Step 1: x**\n\n\
                 ## Activity Log\n\n- 2026-08-02T10:00Z — started\n";
        assert!(lint_description(d).is_empty(), "{:?}", lint_description(d));
    }

    #[test]
    fn empty_description_is_clean() {
        assert!(lint_description("").is_empty());
    }

    #[test]
    fn bad_task_heading_is_an_error() {
        let d = "## Plan\n\n### Step one\n\n- [ ] x\n";
        assert_eq!(errors(d), 1);
    }

    #[test]
    fn malformed_checkbox_is_an_error() {
        // Uppercase X at column zero: looks ticked, tick/find_step disagree.
        let d = "## Plan\n\n### Task 1: a\n\n- [X] step\n";
        assert_eq!(errors(d), 1);
    }

    #[test]
    fn indented_checkbox_is_a_warning_not_an_error() {
        // Child bullets are legitimate, but a checkbox down there is
        // unreachable by tick — flag without failing.
        let d = "## Plan\n\n### Task 1: a\n\n- [ ] real step\n  - [ ] hidden\n";
        assert_eq!(errors(d), 0);
        assert!(lint_description(d)
            .iter()
            .any(|f| f.severity == Severity::Warning && f.message.contains("indented")));
    }

    #[test]
    fn skipped_task_number_is_a_warning() {
        let d = "## Plan\n\n### Task 1: a\n\n- [ ] x\n\n### Task 3: b\n\n- [ ] y\n";
        let f = lint_description(d);
        assert!(f.iter().any(|f| f.severity == Severity::Warning));
        assert_eq!(errors(d), 0);
    }

    #[test]
    fn flat_checklist_without_tasks_is_flagged() {
        // Lints structurally clean, but tick can't address a single step.
        let d = "## Plan\n\n- [ ] first\n- [ ] second\n";
        let f = lint_description(d);
        assert!(
            f.iter().any(|f| f.message.contains("tick cannot reach")),
            "{f:?}"
        );
    }

    #[test]
    fn steps_inside_tasks_are_not_flagged_as_unreachable() {
        let d = "## Plan\n\n### Task 1: a\n\n- [ ] in task\n";
        assert!(!lint_description(d)
            .iter()
            .any(|f| f.message.contains("tick cannot reach")));
    }

    #[test]
    fn stepless_task_is_a_warning() {
        let d = "## Plan\n\n### Task 1: a\n\nprose only\n";
        let f = lint_description(d);
        assert!(f
            .iter()
            .any(|f| f.severity == Severity::Warning && f.message.contains("no steps")));
    }

    #[test]
    fn unparseable_activity_line_is_a_warning() {
        let d = "## Activity Log\n\n- yesterday: did stuff\n";
        let f = lint_description(d);
        assert_eq!(f.len(), 2, "{f:?}"); // missing-spec warning + activity warning
        assert!(f.iter().all(|f| f.severity == Severity::Warning));
    }

    #[test]
    fn a_sublist_under_a_log_entry_is_not_a_malformed_entry() {
        // The reported bug: detail bullets beneath an entry drew one spurious
        // "does not parse" warning each.
        let d = "## Spec\n\ns\n\n## Activity Log\n\n- 2026-08-08T10:00Z — did a thing\n  \
                 - detail one\n  - detail two\n";
        let f = lint_description(d);
        assert!(f.is_empty(), "sublists are entry body, not entries: {f:?}");
    }

    #[test]
    fn a_wrapped_log_entry_is_not_flagged() {
        let d = "## Spec\n\ns\n\n## Activity Log\n\n- 2026-08-08T10:00Z — a long entry that\n  \
                 wraps onto another line\n";
        assert!(lint_description(d).is_empty(), "{:?}", lint_description(d));
    }

    #[test]
    fn a_fenced_plan_is_content_not_structure() {
        // A spec-shaped plan quoting the contract used to draw a malformed
        // checkbox error and a bogus task heading error from inside a fence.
        let d = "## Spec\n\ns\n\n## Plan\n\n### Task 1: a\n\n- [ ] **Step 1: x**\n\n\
                 The format is:\n\n```markdown\n### Step one\n\n- [X] not a real checkbox\n```\n";
        let f = lint_description(d);
        assert!(f.is_empty(), "fenced content must not lint: {f:?}");
    }

    #[test]
    fn steps_after_a_fenced_block_still_count_as_inside_the_task() {
        let d = "## Spec\n\ns\n\n## Plan\n\n### Task 1: a\n\n```\n### Task 9: fake\n```\n\n\
                 - [ ] **Step 1: x**\n";
        let f = lint_description(d);
        assert!(
            !f.iter().any(|f| f.message.contains("tick cannot reach")),
            "{f:?}"
        );
    }
}
