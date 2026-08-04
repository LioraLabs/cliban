//! Parses and mutates the cliban issue/milestone description markdown contract.
//!
//! All functions are pure: input string in, output string + error out. The store
//! layer wraps these in SQL transactions so mutations are atomic.

use chrono::{DateTime, Utc};

/// Timestamp format for `## Activity Log` entries: RFC-3339 with minute
/// precision, UTC, "Z" suffix.
const ACTIVITY_LOG_TIME_FORMAT: &str = "%Y-%m-%dT%H:%MZ";

/// Locating an H2 section now lives in `cliban_core::sections`, because the
/// Linear bridge needs the identical boundaries when it replaces `## Spec`
/// without disturbing `## Plan`. Re-exported here so every call site in this
/// crate reads the same as it always did.
pub use cliban_core::sections::find_section;

/// Constructs a descmd error string with the structured `descmd: ` prefix.
fn errf(msg: String) -> String {
    format!("descmd: {msg}")
}

/// Locates the N-th task within a plan-section body. Tasks are identified by an
/// H3 heading of the form "### Task <N>:" at the start of a line. The matching
/// is exact: a search for Task 1 will not match "### Task 10:" (the trailing
/// colon in the prefix prevents that). Returns the [start, end) byte offsets of
/// the task's body — content AFTER the heading line, up to (but excluding) the
/// next "### " heading or end of input.
pub fn find_task(plan_body: &str, n: i32) -> (usize, usize, bool) {
    let prefix = format!("### Task {n}:");
    let mut offset = 0usize;
    let mut task_body_start: Option<usize> = None;
    for line in plan_body.split_inclusive('\n') {
        let line_len = line.len();
        let trimmed = line.trim_end_matches(['\r', '\n']);
        match task_body_start {
            None => {
                if trimmed.starts_with(&prefix) {
                    task_body_start = Some(offset + line_len);
                }
            }
            Some(start) => {
                if trimmed.starts_with("### ") {
                    return (start, offset, true);
                }
            }
        }
        offset += line_len;
    }
    match task_body_start {
        None => (0, 0, false),
        Some(start) => (start, plan_body.len(), true),
    }
}

/// One bite-sized step line in a Task body.
pub struct Step {
    /// 1-based step index within the task; reserved for future step-level
    /// mutations.
    #[allow(dead_code)]
    pub index: i32,
    /// Current checkbox state.
    pub checked: bool,
    /// Byte offset of the line start within the task body.
    pub line_start: usize,
    /// Byte offset just past the trailing newline (or len(task) if last).
    /// Reserved for future step-level mutations.
    #[allow(dead_code)]
    pub line_end: usize,
    /// The full line content including the trailing newline (and \r if input
    /// uses CRLF) — preserved verbatim for round-trip mutations.
    pub raw: String,
}

/// Locates the M-th step line in a task body. Steps are top-level GFM checkbox
/// list items: lines beginning with "- [ ] " or "- [x] " at column zero.
/// Indented child bullets are ignored.
pub fn find_step(task_body: &str, m: i32) -> Option<Step> {
    let mut offset = 0usize;
    let mut count = 0i32;
    for line in task_body.split_inclusive('\n') {
        let line_len = line.len();
        if line.starts_with("- [ ] ") || line.starts_with("- [x] ") {
            count += 1;
            if count == m {
                return Some(Step {
                    index: m,
                    checked: line.starts_with("- [x] "),
                    line_start: offset,
                    line_end: offset + line_len,
                    raw: line.to_string(),
                });
            }
        }
        offset += line_len;
    }
    None
}

/// The two success shapes of [`tick_step`]: the step was flipped, or it was
/// already in the desired state. An already-checked step is NOT an error — a
/// caller retrying after a timeout is asking for a state the plan already has —
/// but it is a distinct, detectable case so the CLI can say "nothing to do"
/// instead of pretending it wrote.
pub enum TickOutcome {
    /// The step was unchecked; here is the rewritten description.
    Ticked(String),
    /// The step is already checked — the description is untouched.
    AlreadyChecked,
}

/// Flips the M-th step of task N in the description from "- [ ] ..." to
/// "- [x] ...". Returns [`TickOutcome::Ticked`] with the rewritten description,
/// or [`TickOutcome::AlreadyChecked`] when the box is already ticked (desired
/// state, no rewrite). Returns an error only for genuinely wrong targets: the
/// `## Plan` section is missing, the task is missing, or the step is missing.
pub fn tick_step(desc: &str, task_n: i32, step_m: i32) -> Result<TickOutcome, String> {
    let (plan_start, plan_end, ok) = find_section(desc, "Plan");
    if !ok {
        return Err(errf("no ## Plan section".to_string()));
    }
    let plan_body = &desc[plan_start..plan_end];
    let (task_start, task_end, ok) = find_task(plan_body, task_n);
    if !ok {
        return Err(errf(format!("no Task {task_n} in ## Plan")));
    }
    let task_body = &plan_body[task_start..task_end];
    let step = match find_step(task_body, step_m) {
        Some(s) => s,
        None => return Err(errf(format!("no Step {step_m} in Task {task_n}"))),
    };
    if step.checked {
        return Ok(TickOutcome::AlreadyChecked);
    }
    // Absolute offset of the step line inside the original desc.
    let abs = plan_start + task_start + step.line_start;
    // The step line is guaranteed to start with "- [ ] ".
    let new_line = format!("- [x] {}", &step.raw["- [ ] ".len()..]);
    Ok(TickOutcome::Ticked(format!(
        "{}{}{}",
        &desc[..abs],
        new_line,
        &desc[abs + step.raw.len()..]
    )))
}

/// Appends a chronological entry to the "## Activity Log" section. The entry is
/// formatted as "- <UTC-ts> — <msg>" with the timestamp rendered as minute
/// precision RFC-3339 UTC. If the section does not exist, one is created at the
/// end of the description.
///
/// Normalization: the resulting section always ends with exactly one trailing
/// newline when Activity Log is the last section in desc, or two trailing
/// newlines when followed by another section (the second newline being the
/// inter-section blank line preserved per markdown convention).
pub fn append_activity_log(desc: &str, msg: &str, ts: DateTime<Utc>) -> String {
    let stamp = ts.format(ACTIVITY_LOG_TIME_FORMAT).to_string();
    let entry = format!("- {stamp} — {msg}\n");
    let (start, end, ok) = find_section(desc, "Activity Log");
    if !ok {
        let sep = if desc.is_empty() {
            ""
        } else if !desc.ends_with('\n') {
            "\n\n"
        } else {
            "\n"
        };
        return format!("{desc}{sep}## Activity Log\n\n{entry}");
    }
    // Insert the entry at the end of the section body. Strip trailing newlines,
    // append the new entry (which ends with \n), then restore the blank-line
    // separator if not the last section.
    let body = &desc[start..end];
    let trimmed = body.trim_end_matches('\n');
    let mut rebuilt = format!("{trimmed}\n{entry}");
    if end < desc.len() {
        // Not the last section: restore the blank-line separator before the
        // following ## heading.
        rebuilt.push('\n');
    }
    format!("{}{}{}", &desc[..start], rebuilt, &desc[end..])
}

/// Reads back the entries [`append_activity_log`] wrote: `- <ts> — <msg>`,
/// oldest first. Lines that don't match that shape (hand-written prose, an
/// entry from some other tool) are skipped rather than guessed at.
pub fn parse_activity_log(desc: &str) -> Vec<(DateTime<Utc>, String)> {
    let (start, end, ok) = find_section(desc, "Activity Log");
    if !ok {
        return Vec::new();
    }
    desc[start..end]
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("- ")?;
            // The separator is an em dash surrounded by spaces; the timestamp
            // may not contain one, so the first occurrence is the split point.
            let (stamp, msg) = rest.split_once(" — ")?;
            let ts = chrono::NaiveDateTime::parse_from_str(
                stamp.trim(),
                // Minute precision, as written; fall back to full RFC3339 for
                // entries someone recorded by hand.
                ACTIVITY_LOG_TIME_FORMAT,
            )
            .map(|t| t.and_utc())
            .or_else(|_| DateTime::parse_from_rfc3339(stamp.trim()).map(|t| t.with_timezone(&Utc)))
            .ok()?;
            Some((ts, msg.trim().to_string()))
        })
        .collect()
}

/// Replaces the M-th step line in task N with the provided `new_line`.
/// `new_line` must end with a single newline character; otherwise an error is
/// returned. The caller is responsible for ensuring `new_line` remains a valid
/// step — the function performs no syntax validation on its content.
pub fn rewrite_step_line(
    desc: &str,
    task_n: i32,
    step_m: i32,
    new_line: &str,
) -> Result<String, String> {
    if !new_line.ends_with('\n') {
        return Err(errf("newLine must end with newline".to_string()));
    }
    let (plan_start, plan_end, ok) = find_section(desc, "Plan");
    if !ok {
        return Err(errf("no ## Plan section".to_string()));
    }
    let plan_body = &desc[plan_start..plan_end];
    let (task_start, task_end, ok) = find_task(plan_body, task_n);
    if !ok {
        return Err(errf(format!("no Task {task_n} in ## Plan")));
    }
    let task_body = &plan_body[task_start..task_end];
    let step = match find_step(task_body, step_m) {
        Some(s) => s,
        None => return Err(errf(format!("no Step {step_m} in Task {task_n}"))),
    };
    let abs = plan_start + task_start + step.line_start;
    Ok(format!(
        "{}{}{}",
        &desc[..abs],
        new_line,
        &desc[abs + step.raw.len()..]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn find_section_returns_content_range() {
        let d = "## Spec\n\nbody\n\n## Plan\n\nplan body\n";
        let (s, e, ok) = find_section(d, "Spec");
        assert!(ok);
        assert_eq!(&d[s..e], "\nbody\n\n");
    }

    #[test]
    fn tick_step_checks_box() {
        let d = "## Plan\n\n### Task 1: x\n\n- [ ] Step 1\n- [ ] Step 2\n";
        let out = match tick_step(d, 1, 1).unwrap() {
            TickOutcome::Ticked(out) => out,
            TickOutcome::AlreadyChecked => panic!("unchecked step must tick"),
        };
        assert!(out.contains("- [x] Step 1"));
        assert!(out.contains("- [ ] Step 2"));
    }

    #[test]
    fn tick_already_checked_is_a_detectable_noop_not_an_error() {
        let d = "## Plan\n\n### Task 1: x\n\n- [x] Step 1\n";
        assert!(matches!(
            tick_step(d, 1, 1),
            Ok(TickOutcome::AlreadyChecked)
        ));
    }

    #[test]
    fn tick_wrong_targets_stay_errors() {
        let d = "## Plan\n\n### Task 1: x\n\n- [x] Step 1\n";
        assert!(tick_step(d, 1, 2).is_err(), "no such step");
        assert!(tick_step(d, 2, 1).is_err(), "no such task");
        assert!(tick_step("no plan here\n", 1, 1).is_err(), "no ## Plan");
    }

    #[test]
    fn append_activity_log_minute_precision() {
        let ts = Utc.with_ymd_and_hms(2026, 6, 19, 14, 46, 30).unwrap();
        let out = append_activity_log("## Spec\n\nbody\n", "hello", ts);
        assert!(
            out.contains("## Activity Log\n\n- 2026-06-19T14:46Z — hello\n"),
            "got {out}"
        );
    }
}
