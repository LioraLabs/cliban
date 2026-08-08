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
pub use cliban_core::sections::{find_section, task_number};

/// Constructs a descmd error string with the structured `descmd: ` prefix.
fn errf(msg: String) -> String {
    format!("descmd: {msg}")
}

/// The `### Task N:` headings of a plan body, in order, as
/// (number, heading range). Anything else at H3 is not a task and is skipped
/// here — `lint` is what reports it.
///
/// Like sections, these come from the markdown grammar rather than a line
/// prefix, so a `### Task 2:` quoted inside a fenced code block in one task's
/// body no longer ends that task.
pub fn task_headings(plan_body: &str) -> Vec<(i32, std::ops::Range<usize>)> {
    cliban_core::sections::h3_headings(plan_body)
        .into_iter()
        .filter_map(|h| task_number(&h.text).map(|n| (n, h.range)))
        .collect()
}

/// Locates the N-th task within a plan-section body. Tasks are identified by an
/// H3 heading of the form "### Task <N>:". Returns the [start, end) byte offsets
/// of the task's body — content AFTER the heading line, up to (but excluding)
/// the next H3 heading or end of input.
pub fn find_task(plan_body: &str, n: i32) -> (usize, usize, bool) {
    // End at the next H3 of any kind, task-shaped or not: a malformed heading
    // still visually opens a new block, and swallowing it into the previous
    // task would let `tick` reach steps the author filed under something else.
    let all_h3 = cliban_core::sections::h3_headings(plan_body);
    for (i, h) in all_h3.iter().enumerate() {
        if task_number(&h.text) == Some(n) {
            let start = h.range.end;
            let end = all_h3
                .get(i + 1)
                .map_or(plan_body.len(), |next| next.range.start);
            return (start, end, true);
        }
    }
    (0, 0, false)
}

/// Number of `### Task N:` headings inside `## Plan` — what lets `promote`
/// infer `--task` when the plan has exactly one.
pub fn count_tasks(desc: &str) -> usize {
    let (start, end, ok) = find_section(desc, "Plan");
    if !ok {
        return 0;
    }
    task_headings(&desc[start..end]).len()
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
/// list items: markdown list items whose first line begins with "- [ ] " or
/// "- [x] ". Child bullets are nested items, not steps, and are ignored.
///
/// `raw` is deliberately only the item's *first line*, not its whole markdown
/// range: a step may own a sublist or a wrapped continuation, and `tick`
/// splices over `raw`. Taking the full item range would flip the checkbox and
/// delete the step's children with it.
pub fn find_step(task_body: &str, m: i32) -> Option<Step> {
    let mut count = 0i32;
    for range in cliban_core::sections::top_level_list_items(task_body) {
        let item = &task_body[range.clone()];
        let first_line_len = item.find('\n').map_or(item.len(), |i| i + 1);
        let raw = &item[..first_line_len];
        if raw.starts_with("- [ ] ") || raw.starts_with("- [x] ") {
            count += 1;
            if count == m {
                return Some(Step {
                    index: m,
                    checked: raw.starts_with("- [x] "),
                    line_start: range.start,
                    line_end: range.start + first_line_len,
                    raw: raw.to_string(),
                });
            }
        }
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

/// One entry in the `## Activity Log` section — one markdown list item.
pub struct ActivityEntry {
    /// The item's first line without its list marker: what an error message
    /// should quote, and where the timestamp lives.
    pub head: String,
    /// Timestamp and message, when the entry has the `- <ts> — <msg>` shape.
    /// `None` for hand-written prose or an entry from some other tool.
    pub parsed: Option<(DateTime<Utc>, String)>,
}

/// Every entry in the `## Activity Log` section, in document order.
///
/// An entry is a *markdown list item*, not a line. That distinction is the
/// whole fix: an indented `- ` beneath an entry is a nested list item and a
/// wrapped prose line beneath one is a lazy continuation — both belong to the
/// entry that opened, and scanning lines made the first look like two more
/// malformed entries and dropped the second on the floor.
///
/// The reader and `lint` both come through here, so they cannot disagree about
/// what an entry is.
pub fn activity_entries(desc: &str) -> Vec<ActivityEntry> {
    let (start, end, ok) = find_section(desc, "Activity Log");
    if !ok {
        return Vec::new();
    }
    let body = &desc[start..end];
    cliban_core::sections::top_level_list_items(body)
        .into_iter()
        .map(|r| {
            let entry = cliban_core::sections::list_item_body(&body[r]);
            let (head, rest) = match entry.split_once('\n') {
                Some((h, r)) => (h.to_string(), Some(r)),
                None => (entry.clone(), None),
            };
            let parsed = parse_entry_head(&head).map(|(ts, msg)| match rest {
                // The body beneath the first line is part of this entry's
                // message: a sublist of detail, or the rest of a wrapped
                // sentence. It is carried through with its nesting intact.
                Some(rest) if !rest.trim().is_empty() => {
                    (ts, format!("{msg}\n{}", rest.trim_end()))
                }
                _ => (ts, msg),
            });
            ActivityEntry { head, parsed }
        })
        .collect()
}

/// `2026-08-08T10:00Z — did a thing` → its timestamp and message.
fn parse_entry_head(head: &str) -> Option<(DateTime<Utc>, String)> {
    // The separator is an em dash surrounded by spaces; the timestamp may not
    // contain one, so the first occurrence is the split point.
    let (stamp, msg) = head.split_once(" — ")?;
    let ts = chrono::NaiveDateTime::parse_from_str(
        stamp.trim(),
        // Minute precision, as written; fall back to full RFC3339 for entries
        // someone recorded by hand.
        ACTIVITY_LOG_TIME_FORMAT,
    )
    .map(|t| t.and_utc())
    .or_else(|_| DateTime::parse_from_rfc3339(stamp.trim()).map(|t| t.with_timezone(&Utc)))
    .ok()?;
    Some((ts, msg.trim().to_string()))
}

/// Reads back the entries [`append_activity_log`] wrote: `- <ts> — <msg>`,
/// oldest first. Entries that don't match that shape are skipped rather than
/// guessed at.
pub fn parse_activity_log(desc: &str) -> Vec<(DateTime<Utc>, String)> {
    activity_entries(desc)
        .into_iter()
        .filter_map(|e| e.parsed)
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

/// The `issue cp` description transform: duplicate the shape, never the
/// history. Keeps only the contract sections `## Spec`, `## Plan`, and
/// `## Notes` — in the order they appear in the source — with the plan body
/// passed through [`reset_plan_body`]. `## Activity Log` and any non-contract
/// H2 section are dropped: they record what happened to the original, not
/// what the work is.
pub fn copy_description(desc: &str) -> String {
    let mut kept: Vec<(usize, &str, &str)> = Vec::new();
    for anchor in ["Spec", "Plan", "Notes"] {
        let (start, end, ok) = find_section(desc, anchor);
        if ok {
            kept.push((start, anchor, desc[start..end].trim_matches('\n')));
        }
    }
    kept.sort_by_key(|(start, _, _)| *start);
    let mut out = String::new();
    for (_, anchor, body) in kept {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str("## ");
        out.push_str(anchor);
        out.push('\n');
        let body = if anchor == "Plan" {
            reset_plan_body(body)
        } else {
            body.to_string()
        };
        if !body.trim().is_empty() {
            out.push('\n');
            out.push_str(body.trim_matches('\n'));
            out.push('\n');
        }
    }
    out
}

/// Reset a plan body to its unstarted shape. Pure transform, two rules:
///   * a column-zero `- [x] ` step becomes `- [ ] ` (indented child boxes are
///     not steps and pass through untouched),
///   * a step's trailing promotion marker ` → KEY` is stripped — but only
///     when the suffix is actually issue-key-shaped, so a step titled
///     "map a → b" keeps its arrow. The marker records that the *original*
///     split that step into an issue; the copy carries no relations, so the
///     pointer would dangle.
pub fn reset_plan_body(plan_body: &str) -> String {
    let mut out = String::with_capacity(plan_body.len());
    for line in plan_body.split_inclusive('\n') {
        if line.starts_with("- [ ] ") || line.starts_with("- [x] ") {
            let rest = &line["- [x] ".len()..];
            out.push_str("- [ ] ");
            out.push_str(&strip_promotion_suffix(rest));
        } else {
            out.push_str(line);
        }
    }
    out
}

/// Strip a trailing ` → KEY` (issue-key-shaped only) from a step line,
/// preserving the line's original newline (if any).
fn strip_promotion_suffix(line: &str) -> String {
    let (body, newline) = match line.strip_suffix('\n') {
        Some(b) => (b.strip_suffix('\r').unwrap_or(b), &line[b.len()..]),
        None => (line, ""),
    };
    let stripped = match body.rfind(" → ") {
        Some(idx) if is_issue_key_shaped(body[idx + " → ".len()..].trim()) => &body[..idx],
        _ => body,
    };
    format!("{}{}", stripped.trim_end(), newline)
}

/// `PROJECT-N`: letters/digits (starting with a letter) before the last dash,
/// a positive integer after it.
pub fn is_issue_key_shaped(s: &str) -> bool {
    let Some(idx) = s.rfind('-') else {
        return false;
    };
    let (project, seq) = (&s[..idx], &s[idx + 1..]);
    !project.is_empty()
        && project
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
        && project.chars().all(|c| c.is_ascii_alphanumeric())
        && !seq.is_empty()
        && seq.chars().all(|c| c.is_ascii_digit())
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
    fn reset_unticks_column_zero_steps_only() {
        let body = "### Task 1: x\n\n- [x] **Step 1: done**\n  - [x] child stays\n- [ ] **Step 2: open**\n  prose\n";
        let out = reset_plan_body(body);
        assert!(out.contains("- [ ] **Step 1: done**"), "got {out:?}");
        assert!(
            out.contains("  - [x] child stays"),
            "indented boxes are not steps: {out:?}"
        );
        assert!(out.contains("- [ ] **Step 2: open**"));
        assert!(out.contains("### Task 1: x"));
    }

    #[test]
    fn reset_strips_key_shaped_promotion_suffixes_only() {
        let body = "- [x] split me → CLI-53\n- [ ] map a → b\n- [x] pipeline x → y → COOK-7\n";
        let out = reset_plan_body(body);
        assert_eq!(
            out,
            "- [ ] split me\n- [ ] map a → b\n- [ ] pipeline x → y\n"
        );
    }

    #[test]
    fn reset_handles_last_line_without_newline() {
        assert_eq!(reset_plan_body("- [x] tail → ABC-12"), "- [ ] tail");
    }

    #[test]
    fn key_shaped_suffix_rules() {
        assert!(is_issue_key_shaped("CLI-53"));
        assert!(is_issue_key_shaped("cook2-101"));
        assert!(!is_issue_key_shaped("b"));
        assert!(!is_issue_key_shaped("a-"));
        assert!(!is_issue_key_shaped("-7"));
        assert!(!is_issue_key_shaped("7-7"));
        assert!(!is_issue_key_shaped("two words-3"));
    }

    #[test]
    fn copy_keeps_contract_sections_in_source_order_and_resets_plan() {
        let d = "## Notes\n\nlesson\n\n## Spec\n\nthe spec\n\n## Plan\n\n### Task 1: t\n\n- [x] done → CLI-9\n- [ ] open\n\n## Activity Log\n\n- 2026-01-01T00:00Z — history\n";
        let out = copy_description(d);
        assert_eq!(
            out,
            "## Notes\n\nlesson\n\n## Spec\n\nthe spec\n\n## Plan\n\n### Task 1: t\n\n- [x] done → CLI-9\n- [ ] open\n"
                .replace("- [x] done → CLI-9", "- [ ] done"),
            "got {out:?}"
        );
        assert!(!out.contains("Activity Log"));
        assert!(!out.contains("history"));
    }

    #[test]
    fn copy_drops_non_contract_sections() {
        let d = "## Spec\n\ns\n\n## Decisions so far\n\n- deliberation\n\n## Plan\n\n- [ ] step\n";
        let out = copy_description(d);
        assert!(!out.contains("Decisions so far"));
        assert!(!out.contains("deliberation"));
        assert!(out.contains("## Spec\n\ns\n"));
        assert!(out.contains("## Plan\n\n- [ ] step\n"));
    }

    #[test]
    fn copy_tolerates_missing_and_empty_sections() {
        assert_eq!(copy_description("free text, no sections\n"), "");
        assert_eq!(copy_description(""), "");
        // An empty section keeps its heading without growing blank runs.
        let out = copy_description("## Spec\n\n## Plan\n\n- [ ] s\n");
        assert_eq!(out, "## Spec\n\n## Plan\n\n- [ ] s\n");
        assert!(!out.contains("\n\n\n"));
    }

    #[test]
    fn an_indented_sublist_is_part_of_its_entry() {
        // Under CommonMark an indented `-` beneath an entry is a nested list
        // item, not a new entry. It used to parse as two more entries that
        // failed the `- <ts> — <msg>` shape and were silently dropped, while
        // lint warned about each one.
        let d = "## Activity Log\n\n- 2026-08-08T10:00Z — did a thing\n  - detail one\n  \
                 - detail two\n- 2026-08-08T11:00Z — next thing\n";
        let out = parse_activity_log(d);
        assert_eq!(out.len(), 2, "two entries, not four: {out:?}");
        assert_eq!(
            out[0].1, "did a thing\n- detail one\n- detail two",
            "the sublist rides along with its entry"
        );
        assert_eq!(out[1].1, "next thing");
    }

    #[test]
    fn a_wrapped_prose_line_folds_into_its_entry() {
        // A plain continuation line is a lazy continuation of the list item.
        // It used to be dropped outright — the message silently lost its tail.
        let d = "## Activity Log\n\n- 2026-08-08T11:00Z — wrapped entry that continues\n  \
                 onto a prose line\n";
        let out = parse_activity_log(d);
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].1,
            "wrapped entry that continues\nonto a prose line",
            "the continuation is not dropped"
        );
    }

    #[test]
    fn an_entry_that_is_not_log_shaped_is_still_reported_once() {
        let d = "## Activity Log\n\n- yesterday: did stuff\n  - with detail\n";
        assert!(parse_activity_log(d).is_empty());
        let entries = activity_entries(d);
        assert_eq!(entries.len(), 1, "one malformed entry, not two");
        assert_eq!(entries[0].head, "yesterday: did stuff");
    }

    #[test]
    fn a_fenced_log_line_is_not_an_entry() {
        let d = "## Activity Log\n\n- 2026-08-08T10:00Z — real\n\n```\n- 2026-01-01T00:00Z — fake\n```\n";
        let out = parse_activity_log(d);
        assert_eq!(out.len(), 1, "the fenced line is code, not an entry: {out:?}");
        assert_eq!(out[0].1, "real");
    }

    #[test]
    fn a_fenced_task_heading_does_not_end_a_task() {
        // Same family as the section bug, one level down: a plan step that
        // quotes the task format used to truncate the task it lives in.
        let plan = "### Task 1: t\n\n- [ ] **Step 1: a**\n- [ ] **Step 2: b**\n\n```markdown\n\
                    ### Task 2: not real\n```\n\n- [ ] **Step 3: c**\n";
        let (s, e, ok) = find_task(plan, 1);
        assert!(ok);
        let body = &plan[s..e];
        assert!(body.contains("Step 3: c"), "task not truncated: {body:?}");
        assert!(find_step(body, 3).is_some(), "the third step is reachable");
        assert_eq!(count_tasks(&format!("## Plan\n\n{plan}")), 1);
    }

    #[test]
    fn ticking_a_step_that_owns_a_sublist_keeps_its_children() {
        // find_step reports only the item's first line precisely so that the
        // tick splice cannot swallow the step's children.
        let d = "## Plan\n\n### Task 1: t\n\n- [ ] **Step 1: a**\n  - note one\n  - note two\n\
                 - [ ] **Step 2: b**\n";
        let out = match tick_step(d, 1, 1).unwrap() {
            TickOutcome::Ticked(out) => out,
            TickOutcome::AlreadyChecked => panic!("unchecked step must tick"),
        };
        assert!(out.contains("- [x] **Step 1: a**"));
        assert!(out.contains("  - note one\n  - note two"), "got {out:?}");
        assert!(out.contains("- [ ] **Step 2: b**"));
    }

    #[test]
    fn a_fenced_checkbox_is_not_a_step() {
        let task = "- [ ] **Step 1: real**\n\n```\n- [ ] fenced, not a step\n```\n\n\
                    - [ ] **Step 2: also real**\n";
        assert!(find_step(task, 1).unwrap().raw.contains("real"));
        assert!(find_step(task, 2).unwrap().raw.contains("also real"));
        assert!(find_step(task, 3).is_none(), "only two steps exist");
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
