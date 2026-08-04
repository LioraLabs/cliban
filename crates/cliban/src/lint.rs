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

    let (act_start, act_end, has_activity) = find_section(desc, "Activity Log");
    if has_activity {
        lint_activity(&desc[act_start..act_end], &mut findings);
    }

    findings
}

fn lint_plan(plan: &str, findings: &mut Vec<Finding>) {
    // Every H3 in the plan must be a task heading; collect the numbers, and
    // track whether we're inside a valid task — a column-zero checkbox outside
    // one lints clean structurally but `tick` can never address it.
    let mut numbers: Vec<i32> = Vec::new();
    let mut in_task = false;
    let mut unreachable_steps = 0usize;
    for line in plan.lines() {
        let trimmed = line.trim_end();
        if let Some(rest) = trimmed.strip_prefix("### ") {
            match rest
                .strip_prefix("Task ")
                .and_then(|r| r.split_once(':'))
                .and_then(|(n, _)| n.trim().parse::<i32>().ok())
            {
                Some(n) => {
                    numbers.push(n);
                    in_task = true;
                }
                None => {
                    err(
                        findings,
                        format!("plan heading {trimmed:?} is not \"### Task N: <title>\""),
                    );
                    in_task = false;
                }
            }
        }
        if !in_task && (line.starts_with("- [ ] ") || line.starts_with("- [x] ")) {
            unreachable_steps += 1;
        }
        // Checkbox-ish lines that tick/find_step will not recognize. At column
        // zero that's a malformed step (error); indented it's a child bullet —
        // legitimate per the contract, but silently untickable, so worth a
        // heads-up when it looks like a checkbox.
        let lstripped = line.trim_start();
        if line.starts_with("- [") && !line.starts_with("- [ ] ") && !line.starts_with("- [x] ") {
            err(
                findings,
                format!("malformed checkbox {trimmed:?} (want \"- [ ] \" or \"- [x] \")"),
            );
        } else if lstripped != line && lstripped.starts_with("- [") {
            warn(
                findings,
                format!(
                    "indented checkbox {:?} is a child bullet — tick cannot reach it",
                    lstripped.trim_end()
                ),
            );
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

fn lint_activity(activity: &str, findings: &mut Vec<Finding>) {
    for line in activity.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("- ") {
            continue;
        }
        let parses = trimmed
            .strip_prefix("- ")
            .and_then(|rest| rest.split_once(" — "))
            .map(|(stamp, _)| {
                chrono::NaiveDateTime::parse_from_str(stamp.trim(), "%Y-%m-%dT%H:%MZ").is_ok()
                    || chrono::DateTime::parse_from_rfc3339(stamp.trim()).is_ok()
            })
            .unwrap_or(false);
        if !parses {
            warn(
                findings,
                format!("activity entry {trimmed:?} does not parse as \"- <ts> — <msg>\""),
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
}
