//! Pure text: what a Linear issue looks like as a cliban description, and what
//! cliban progress looks like in Linear.
//!
//! Everything here is `&str` in, `String` out. No network, no database — which
//! is what makes the merge rules testable, and the merge rules are the part
//! that can quietly destroy someone's work.

use cliban_core::schema::ActivityLogEntry;
use cliban_core::{sections, time};

use super::model;

/// Anchors in the cliban description contract.
pub const SPEC: &str = "Spec";
pub const PLAN: &str = "Plan";

/// Markers delimiting the region of a Linear description that cliban owns.
/// HTML comments, so they are invisible in Linear's rendered view — a human
/// reading the issue sees the content, not the plumbing.
pub const FENCE_BEGIN_PREFIX: &str = "<!-- cliban:begin";
pub const FENCE_END: &str = "<!-- cliban:end -->";

/// The `## Spec` body for an imported issue: Linear's description verbatim,
/// with a provenance line so anyone reading the cliban issue can get back to
/// the source.
pub fn spec_body(issue: &model::Issue) -> String {
    let desc = issue.description_text();
    let provenance = format!("_Imported from [{}]({})._", issue.identifier, issue.url);
    if desc.is_empty() {
        provenance
    } else {
        format!("{desc}\n\n{provenance}")
    }
}

/// The full description for a newly imported issue: the spec, plus an empty
/// `## Plan` so `cliban issue tick` has a section to work against immediately
/// rather than failing with "no ## Plan section".
pub fn initial_description(issue: &model::Issue) -> String {
    format!(
        "## Spec\n\n{}\n\n## Plan\n\n_No plan yet. Add tasks as `### Task N: title` \
         with `- [ ] **Step M: ...**` steps._\n",
        spec_body(issue)
    )
}

/// Re-import: refresh the Linear-owned `## Spec` and leave every other section
/// byte-identical. This is the load-bearing guarantee of the whole bridge — an
/// agent's half-ticked `## Plan` must survive a refresh.
pub fn refresh_description(existing: &str, issue: &model::Issue) -> String {
    sections::replace_section(existing, SPEC, &spec_body(issue))
}

/// Splice `inner` into the cliban-owned fenced region of a Linear description,
/// leaving all surrounding prose untouched. Appends the region when absent.
///
/// A begin marker with no matching end is treated as running to the end of the
/// description. That recovers from a human deleting the end marker without
/// duplicating the block, which is the failure mode that would actually happen.
pub fn apply_fence(existing: &str, cliban_key: &str, inner: &str) -> String {
    let block = format!(
        "{FENCE_BEGIN_PREFIX} {cliban_key} -->\n{}\n{FENCE_END}",
        inner.trim_end()
    );

    match fence_range(existing) {
        Some((start, end)) => {
            let mut out = String::with_capacity(existing.len() + block.len());
            out.push_str(&existing[..start]);
            out.push_str(&block);
            out.push_str(&existing[end..]);
            out
        }
        None => {
            let base = existing.trim_end();
            if base.is_empty() {
                format!("{block}\n")
            } else {
                format!("{base}\n\n{block}\n")
            }
        }
    }
}

/// Byte range of the existing fenced block, markers included.
fn fence_range(desc: &str) -> Option<(usize, usize)> {
    let start = find_line_starting_with(desc, FENCE_BEGIN_PREFIX)?;
    let after_begin = &desc[start..];
    match find_line_starting_with(after_begin, FENCE_END) {
        Some(rel) => {
            let end_line_start = start + rel;
            let end = end_line_start + line_len_at(desc, end_line_start);
            Some((start, end))
        }
        // Begin without end: cliban owns everything from the marker onward.
        None => Some((start, desc.len())),
    }
}

/// Byte offset of the first line whose trimmed start matches `prefix`.
fn find_line_starting_with(s: &str, prefix: &str) -> Option<usize> {
    let mut offset = 0usize;
    for line in s.split_inclusive('\n') {
        if line.trim_start().starts_with(prefix) {
            return Some(offset);
        }
        offset += line.len();
    }
    None
}

/// Length of the line beginning at `at`, trailing newline included.
fn line_len_at(s: &str, at: usize) -> usize {
    s[at..]
        .split_inclusive('\n')
        .next()
        .map(str::len)
        .unwrap_or(0)
}

/// One task's tick count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskProgress {
    pub number: i32,
    pub title: String,
    pub done: usize,
    pub total: usize,
}

/// Count ticked steps per task in a `## Plan` body.
///
/// Read-only, and deliberately mirrors the CLI `descmd` step rule rather than
/// inventing its own: a step is a GFM checkbox at **column zero**, so indented
/// child bullets are prose, not steps. Getting this wrong would only misreport
/// a comment, never corrupt a description — which is why counting lives here
/// while mutation stays in `descmd`.
pub fn plan_progress(plan_body: &str) -> Vec<TaskProgress> {
    let mut out: Vec<TaskProgress> = Vec::new();
    for line in plan_body.lines() {
        if let Some(rest) = line.strip_prefix("### Task ") {
            let (number, title) = split_task_heading(rest);
            if let Some(number) = number {
                out.push(TaskProgress {
                    number,
                    title,
                    done: 0,
                    total: 0,
                });
            }
            continue;
        }
        let checked = if line.starts_with("- [x] ") || line.starts_with("- [X] ") {
            Some(true)
        } else if line.starts_with("- [ ] ") {
            Some(false)
        } else {
            None
        };
        if let (Some(checked), Some(task)) = (checked, out.last_mut()) {
            task.total += 1;
            if checked {
                task.done += 1;
            }
        }
    }
    out
}

/// `1: short title` → `(Some(1), "short title")`.
fn split_task_heading(rest: &str) -> (Option<i32>, String) {
    match rest.split_once(':') {
        Some((num, title)) => (num.trim().parse().ok(), title.trim().to_string()),
        None => (None, String::new()),
    }
}

/// The comment `push` posts on the Linear issue: where the plan stands and
/// what happened since the last sync.
///
/// Additive by construction — a comment can never destroy anything a human
/// wrote, which is why it is on by default while the description rewrite is
/// opt-in.
pub fn progress_comment(
    cliban_key: &str,
    status: &str,
    plan_body: Option<&str>,
    activity: &[ActivityLogEntry],
) -> String {
    let mut out = format!("**cliban `{cliban_key}`** — status: `{status}`\n");

    if let Some(plan) = plan_body {
        let tasks = plan_progress(plan);
        let done: usize = tasks.iter().map(|t| t.done).sum();
        let total: usize = tasks.iter().map(|t| t.total).sum();
        if total > 0 {
            out.push_str(&format!("\n**Plan: {done}/{total} steps**\n\n"));
            for task in &tasks {
                let check = if task.total > 0 && task.done == task.total {
                    "x"
                } else {
                    " "
                };
                out.push_str(&format!(
                    "- [{check}] Task {}: {} — {}/{}\n",
                    task.number, task.title, task.done, task.total
                ));
            }
        }
    }

    if !activity.is_empty() {
        out.push_str("\n**Activity**\n\n");
        for entry in activity {
            out.push_str(&format!(
                "- `{}` {} — {}\n",
                time::format_usec(entry.ts),
                entry.kind,
                entry.message.trim()
            ));
        }
    }

    out
}

/// How many `issue log` findings the living digest keeps. Roughly "what
/// happened lately", not a transcript — the full log lives on the board.
const DIGEST_FINDINGS: usize = 3;

/// The body of the *living* progress comment: one comment per linked issue,
/// rewritten in place on every push, so it always describes now.
///
/// Unlike [`progress_comment`] — which appends what changed since the last
/// sync — this is a snapshot: plan progress, the last few `issue log`
/// findings, the latest test status a finding carried, and a footer telling
/// a human why the comment keeps changing under them.
pub fn progress_digest(
    cliban_key: &str,
    status: &str,
    plan_body: Option<&str>,
    activity: &[ActivityLogEntry],
) -> String {
    let mut out = format!("**cliban `{cliban_key}`** — status: `{status}`\n");

    if let Some(plan) = plan_body {
        let tasks = plan_progress(plan);
        let done: usize = tasks.iter().map(|t| t.done).sum();
        let total: usize = tasks.iter().map(|t| t.total).sum();
        if total > 0 {
            out.push_str(&format!("\n**Plan: {done}/{total} steps**\n\n"));
            for task in &tasks {
                let check = if task.total > 0 && task.done == task.total {
                    "x"
                } else {
                    " "
                };
                out.push_str(&format!(
                    "- [{check}] Task {}: {} — {}/{}\n",
                    task.number, task.title, task.done, task.total
                ));
            }
        }
    }

    // Findings are what an agent chose to say (`issue log`), not what the tool
    // recorded about itself — status moves and ticks are already visible as
    // plan progress and the state itself.
    let findings: Vec<&ActivityLogEntry> = activity.iter().filter(|e| e.kind == "log").collect();

    if let Some(status_line) = findings.iter().rev().find_map(|e| test_status(&e.message)) {
        out.push_str(&format!("\n**Tests:** {status_line}\n"));
    }

    if !findings.is_empty() {
        out.push_str("\n**Latest findings**\n\n");
        let skip = findings.len().saturating_sub(DIGEST_FINDINGS);
        for entry in &findings[skip..] {
            out.push_str(&format!(
                "- `{}` — {}\n",
                time::format_usec(entry.ts),
                entry.message.trim()
            ));
        }
    }

    out.push_str(
        "\n---\n_This comment is maintained by cliban and updated in place on each push._\n",
    );
    out
}

/// Pull a test status like `447 passed / 0 failed` out of a log message, if it
/// carries one. A count is a number immediately before the word `passed` or
/// `failed`; punctuation around tokens is ignored. Heuristic on purpose — a
/// miss only omits a digest line, it never corrupts anything.
fn test_status(message: &str) -> Option<String> {
    let mut passed: Option<u64> = None;
    let mut failed: Option<u64> = None;
    let tokens: Vec<&str> = message
        .split_whitespace()
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric()))
        .collect();
    for pair in tokens.windows(2) {
        let Ok(n) = pair[0].parse::<u64>() else {
            continue;
        };
        match pair[1].to_ascii_lowercase().as_str() {
            "passed" => passed = Some(n),
            "failed" => failed = Some(n),
            _ => {}
        }
    }
    let mut parts = Vec::new();
    if let Some(n) = passed {
        parts.push(format!("{n} passed"));
    }
    if let Some(n) = failed {
        parts.push(format!("{n} failed"));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" / "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(desc: Option<&str>) -> model::Issue {
        serde_json::from_value(serde_json::json!({
            "id": "uuid-1",
            "identifier": "ENG-412",
            "title": "Fix the thing",
            "description": desc,
            "url": "https://linear.app/acme/issue/ENG-412",
            "updatedAt": "2026-07-29T12:00:00.000Z",
            "priority": 2,
            "dueDate": null,
            "state": {"id": "s1", "name": "Todo", "type": "unstarted", "position": 1.0},
            "team": {"id": "t1", "key": "ENG", "name": "Engineering"},
            "labels": {"nodes": []}
        }))
        .unwrap()
    }

    #[test]
    fn spec_body_carries_the_description_and_a_link_home() {
        let body = spec_body(&issue(Some("The bug is real.")));
        assert!(body.starts_with("The bug is real."));
        assert!(body.contains("[ENG-412](https://linear.app/acme/issue/ENG-412)"));
    }

    #[test]
    fn spec_body_of_an_empty_issue_is_just_provenance() {
        let body = spec_body(&issue(None));
        assert!(body.starts_with("_Imported from"));
        assert!(!body.starts_with("\n"));
    }

    #[test]
    fn initial_description_has_both_contract_sections() {
        let desc = initial_description(&issue(Some("spec text")));
        assert!(sections::find_section(&desc, SPEC).2);
        assert!(sections::find_section(&desc, PLAN).2, "tick needs ## Plan");
    }

    #[test]
    fn refresh_replaces_spec_and_preserves_a_half_ticked_plan() {
        let existing = "## Spec\n\nold spec\n\n## Plan\n\n### Task 1: wire it\n\n\
                        - [x] **Step 1: done**\n- [ ] **Step 2: todo**\n\n\
                        ## Activity Log\n\n2026-07-01T10:00Z  note  started\n";
        let out = refresh_description(existing, &issue(Some("NEW spec")));

        assert!(out.contains("NEW spec"));
        assert!(!out.contains("old spec"));
        // The two things a refresh must never touch.
        assert!(out.contains("- [x] **Step 1: done**"));
        assert!(out.contains("- [ ] **Step 2: todo**"));
        assert!(out.contains("2026-07-01T10:00Z  note  started"));
    }

    #[test]
    fn refresh_is_idempotent() {
        let existing = "## Spec\n\nold\n\n## Plan\n\n### Task 1: x\n\n- [ ] **Step 1: a**\n";
        let once = refresh_description(existing, &issue(Some("new")));
        let twice = refresh_description(&once, &issue(Some("new")));
        assert_eq!(once, twice);
    }

    #[test]
    fn fence_appends_when_absent_and_leaves_prose_alone() {
        let out = apply_fence("Human wrote this.\n", "PROJ-42", "plan mirror");
        assert!(out.starts_with("Human wrote this."));
        assert!(out.contains("<!-- cliban:begin PROJ-42 -->"));
        assert!(out.contains("plan mirror"));
        assert!(out.contains(FENCE_END));
    }

    #[test]
    fn fence_replaces_only_its_own_region() {
        let existing = "Above the fence.\n\n\
                        <!-- cliban:begin PROJ-42 -->\nOLD MIRROR\n<!-- cliban:end -->\n\n\
                        Below the fence.\n";
        let out = apply_fence(existing, "PROJ-42", "NEW MIRROR");
        assert!(out.contains("Above the fence."));
        assert!(out.contains("Below the fence."));
        assert!(out.contains("NEW MIRROR"));
        assert!(!out.contains("OLD MIRROR"));
    }

    #[test]
    fn fence_is_idempotent_and_does_not_multiply() {
        let mut desc = "Human prose.\n".to_string();
        for _ in 0..3 {
            desc = apply_fence(&desc, "PROJ-42", "mirror");
        }
        assert_eq!(desc.matches(FENCE_BEGIN_PREFIX).count(), 1);
        assert_eq!(desc.matches(FENCE_END).count(), 1);
        assert_eq!(desc.matches("Human prose.").count(), 1);
    }

    #[test]
    fn fence_recovers_from_a_deleted_end_marker_without_duplicating() {
        let existing = "Prose.\n\n<!-- cliban:begin PROJ-42 -->\nstale mirror\n";
        let out = apply_fence(existing, "PROJ-42", "fresh");
        assert_eq!(out.matches(FENCE_BEGIN_PREFIX).count(), 1);
        assert!(out.contains("fresh"));
        assert!(!out.contains("stale mirror"));
        assert!(out.contains("Prose."));
    }

    #[test]
    fn plan_progress_counts_only_column_zero_checkboxes() {
        let plan = "\n### Task 1: setup\n\n\
                    - [x] **Step 1: a**\n\
                    - [x] **Step 2: b**\n\
                    \x20 - [ ] an indented child bullet, not a step\n\n\
                    ### Task 2: build\n\n\
                    - [ ] **Step 1: c**\n";
        let tasks = plan_progress(plan);
        assert_eq!(tasks.len(), 2);
        assert_eq!(
            tasks[0],
            TaskProgress {
                number: 1,
                title: "setup".into(),
                done: 2,
                total: 2
            }
        );
        assert_eq!(
            tasks[1],
            TaskProgress {
                number: 2,
                title: "build".into(),
                done: 0,
                total: 1
            }
        );
    }

    #[test]
    fn plan_progress_of_an_empty_plan_is_empty_not_a_panic() {
        assert!(plan_progress("").is_empty());
        assert!(plan_progress("\n_No plan yet._\n").is_empty());
    }

    #[test]
    fn plan_progress_ignores_steps_before_any_task_heading() {
        // Malformed, but it must not panic on `out.last_mut()` being None.
        assert!(plan_progress("- [x] **orphan step**\n").is_empty());
    }

    #[test]
    fn progress_comment_reports_totals_and_marks_finished_tasks() {
        let plan = "\n### Task 1: setup\n\n- [x] **Step 1: a**\n\n\
                    ### Task 2: build\n\n- [ ] **Step 1: b**\n";
        let out = progress_comment("PROJ-42", "in-progress", Some(plan), &[]);
        assert!(out.contains("**cliban `PROJ-42`**"));
        assert!(out.contains("Plan: 1/2 steps"));
        assert!(out.contains("- [x] Task 1: setup — 1/1"));
        assert!(out.contains("- [ ] Task 2: build — 0/1"));
    }

    #[test]
    fn progress_comment_without_a_plan_still_reports_status() {
        let out = progress_comment("PROJ-42", "done", None, &[]);
        assert!(out.contains("status: `done`"));
        assert!(!out.contains("Plan:"));
    }

    // ---- the living digest ----

    fn entry(ts: &str, kind: &str, message: &str) -> ActivityLogEntry {
        ActivityLogEntry {
            id: 0,
            issue_id: 1,
            ts: time::parse_ts(ts).unwrap(),
            kind: kind.into(),
            message: message.into(),
            extra: "{}".into(),
            inserted_at: time::parse_ts(ts).unwrap(),
            updated_at: time::parse_ts(ts).unwrap(),
        }
    }

    #[test]
    fn digest_reports_plan_progress_and_the_footer() {
        let plan = "\n### Task 1: setup\n\n- [x] **Step 1: a**\n- [ ] **Step 2: b**\n";
        let out = progress_digest("PROJ-42", "in-progress", Some(plan), &[]);
        assert!(out.contains("**cliban `PROJ-42`**"));
        assert!(out.contains("Plan: 1/2 steps"));
        assert!(
            out.contains("maintained by cliban"),
            "the footer tells a human why the comment keeps changing:\n{out}"
        );
    }

    #[test]
    fn digest_keeps_only_the_last_three_log_findings_and_skips_bookkeeping() {
        let activity = vec![
            entry("2026-08-01T10:00:00Z", "log", "finding one"),
            entry("2026-08-01T10:01:00Z", "status", "backlog → in-progress"),
            entry("2026-08-01T10:02:00Z", "log", "finding two"),
            entry("2026-08-01T10:03:00Z", "tick", "ticked Task 1 Step 1"),
            entry("2026-08-01T10:04:00Z", "log", "finding three"),
            entry("2026-08-01T10:05:00Z", "log", "finding four"),
        ];
        let out = progress_digest("PROJ-42", "in-progress", None, &activity);
        assert!(
            !out.contains("finding one"),
            "oldest finding must age out:\n{out}"
        );
        assert!(out.contains("finding two"));
        assert!(out.contains("finding three"));
        assert!(out.contains("finding four"));
        assert!(
            !out.contains("backlog → in-progress") && !out.contains("ticked Task 1"),
            "bookkeeping entries are not findings:\n{out}"
        );
    }

    #[test]
    fn digest_surfaces_the_latest_test_status_a_log_entry_carries() {
        let activity = vec![
            entry("2026-08-01T10:00:00Z", "log", "suite red: 3 failed"),
            entry(
                "2026-08-01T11:00:00Z",
                "log",
                "green again: 447 passed / 0 failed",
            ),
        ];
        let out = progress_digest("PROJ-42", "in-progress", None, &activity);
        assert!(
            out.contains("**Tests:** 447 passed / 0 failed"),
            "the newest test status should be pulled out on its own line:\n{out}"
        );
    }

    #[test]
    fn digest_without_test_talk_has_no_tests_line() {
        let activity = vec![entry("2026-08-01T10:00:00Z", "log", "wired the client")];
        let out = progress_digest("PROJ-42", "in-progress", None, &activity);
        assert!(!out.contains("**Tests:**"), "{out}");
    }

    #[test]
    fn digest_of_a_bare_issue_is_still_valid_markdown_with_status_and_footer() {
        let out = progress_digest("PROJ-42", "backlog", None, &[]);
        assert!(out.contains("status: `backlog`"));
        assert!(out.contains("maintained by cliban"));
        assert!(!out.contains("Plan:"));
        assert!(!out.contains("**Latest findings**"));
    }
}
