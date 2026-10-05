//! `--brief`: a short plain-text summary meant to be read aloud or pasted into
//! a prompt. Pure renderers over plain data; no markdown, no tables.

use chrono::{DateTime, Utc};
use cliban_core::schema::Issue;
use cliban_core::sections::find_section;
use cliban_core::time::relative;

const MAX_LINES: usize = 40;
const MAX_WIDTH: usize = 200;

/// Truncate to `max` characters (not bytes), marking the cut.
pub fn elide(s: &str, max: usize) -> String {
    let flat = s.replace('\n', " ");
    if flat.chars().count() <= max {
        return flat;
    }
    flat.chars()
        .take(max.saturating_sub(1))
        .chain(['…'])
        .collect()
}

/// The one cap: every line elided, at most 40 lines (39 plus "And N more.").
pub fn cap(mut lines: Vec<String>) -> String {
    if lines.len() > MAX_LINES {
        let more = lines.len() - (MAX_LINES - 1);
        lines.truncate(MAX_LINES - 1);
        lines.push(format!("And {more} more."));
    }
    let mut out = String::new();
    for l in lines {
        out.push_str(&elide(&l, MAX_WIDTH));
        out.push('\n');
    }
    out
}

/// End a fragment with exactly one period.
fn sentence(s: &str) -> String {
    format!("{}.", s.trim().trim_end_matches(['.', '!', '?']))
}

/// First two sentences of the issue's spec, markdown stripped.
fn gist(desc: &str) -> String {
    let (start, end, found) = find_section(desc, "Spec");
    let text = if found {
        &desc[start..end]
    } else {
        // Everything before the first H2.
        let cut = if desc.starts_with("## ") {
            0
        } else {
            desc.find("\n## ").unwrap_or(desc.len())
        };
        &desc[..cut]
    };
    let mut words: Vec<String> = Vec::new();
    for line in text.lines() {
        let l = line.trim_start();
        if l.starts_with('#') {
            continue;
        }
        let l = l
            .trim_start_matches(['-', '*', '+', ' '])
            .trim_start_matches("[ ]")
            .trim_start_matches("[x]")
            .trim_start_matches("[X]")
            .replace(['*', '`'], "");
        words.extend(l.split_whitespace().map(str::to_string));
    }
    let flat = words.join(" ");
    let mut seen = 0;
    let mut cut = flat.len();
    let chars: Vec<(usize, char)> = flat.char_indices().collect();
    for (n, &(i, c)) in chars.iter().enumerate() {
        if matches!(c, '.' | '!' | '?') && chars.get(n + 1).is_none_or(|&(_, d)| d == ' ') {
            seen += 1;
            if seen == 2 {
                cut = i;
                break;
            }
        }
    }
    elide(&flat[..cut], MAX_WIDTH - 20)
}

/// Everything `issue show --brief` needs, already fetched.
pub struct IssueBrief<'a> {
    pub issue: &'a Issue,
    pub milestone: Option<&'a str>,
    /// Open blockers: (key, status).
    pub blockers: &'a [(String, String)],
    pub claimed_by: Option<&'a str>,
}

pub fn issue(b: &IssueBrief, now: DateTime<Utc>) -> String {
    let i = b.issue;
    let mut status = format!("Status {}", i.status);
    if i.priority != "none" {
        status += &format!(", priority {}", i.priority);
    }
    if let Some(m) = b.milestone.filter(|m| !m.is_empty()) {
        status += &format!(", milestone {m}");
    }
    let mut lines = vec![
        sentence(&format!("{}: {}", i.key, i.title)),
        sentence(&status),
    ];
    let g = gist(&i.description);
    if !g.is_empty() {
        lines.push(sentence(&format!("Spec: {g}")));
    }
    lines.push(if b.blockers.is_empty() {
        "No open blockers.".into()
    } else {
        let list: Vec<String> = b
            .blockers
            .iter()
            .map(|(k, s)| format!("{k} ({s})"))
            .collect();
        sentence(&format!("Blocked by {}", list.join(", ")))
    });
    let newest = crate::descmd::parse_activity_log(&i.description)
        .into_iter()
        .max_by_key(|(ts, _)| *ts);
    lines.push(match newest {
        Some((ts, msg)) => sentence(&format!(
            "Latest log {}: {}",
            relative(ts, now),
            elide(&msg, 120)
        )),
        None => "No log entries.".into(),
    });
    if let Some(c) = b.claimed_by {
        lines.push(sentence(&format!("Claimed by {c}")));
    }
    lines.push(sentence(&format!(
        "Last updated {}",
        relative(i.updated_at, now)
    )));
    cap(lines)
}

/// Board summary over `issues`, under a caller-supplied header sentence.
pub fn board(header: &str, issues: &[Issue], now: DateTime<Utc>) -> String {
    let mut sorted: Vec<&Issue> = issues.iter().collect();
    sorted.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.key.cmp(&b.key))
    });
    let mut lines = vec![header.to_string()];
    if sorted.is_empty() {
        lines.push("No issues.".into());
        return cap(lines);
    }
    let parts: Vec<String> = ["in-progress", "blocked", "in-review", "backlog", "done"]
        .iter()
        .filter_map(|s| {
            let n = sorted.iter().filter(|i| i.status == *s).count();
            let label = if *s == "backlog" {
                "in backlog".into()
            } else {
                s.replace('-', " ")
            };
            (n > 0).then(|| format!("{n} {label}"))
        })
        .collect();
    let n = sorted.len();
    lines.push(format!(
        "{n} issue{}: {}.",
        if n == 1 { "" } else { "s" },
        parts.join(", ")
    ));
    for (status, label) in [
        ("in-progress", "In progress"),
        ("blocked", "Blocked"),
        ("in-review", "In review"),
    ] {
        for i in sorted.iter().filter(|i| i.status == status) {
            lines.push(format!(
                "{label}: {} {}, updated {}.",
                i.key,
                i.title.trim_end_matches('.'),
                relative(i.updated_at, now)
            ));
        }
    }
    let i = sorted[0];
    lines.push(format!(
        "Newest change {}: {} {}, now {}.",
        relative(i.updated_at, now),
        i.key,
        i.title.trim_end_matches('.'),
        i.status
    ));
    cap(lines)
}

/// Activity feed: `(ts, key, text)` newest first.
pub fn feed(since: &str, events: &[(DateTime<Utc>, String, String)], now: DateTime<Utc>) -> String {
    let n = events.len();
    let mut lines = vec![if n == 0 {
        sentence(&format!("No changes since {since}"))
    } else {
        sentence(&format!(
            "{n} change{} since {since}",
            if n == 1 { "" } else { "s" }
        ))
    }];
    for (ts, key, text) in events {
        lines.push(sentence(&format!("{}, {key}: {text}", relative(*ts, now))));
    }
    cap(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 5, 12, 0, 0).unwrap()
    }

    fn iss(key: &str, status: &str, hours_ago: i64, desc: &str) -> Issue {
        let t = now() - chrono::Duration::hours(hours_ago);
        Issue {
            id: 1,
            key: key.into(),
            project_id: 1,
            milestone_id: None,
            parent_id: None,
            title: format!("Title {key}"),
            description: desc.into(),
            status: status.into(),
            priority: "high".into(),
            position: 0.0,
            archived: false,
            due_date: None,
            completed_at: None,
            inserted_at: t,
            updated_at: t,
        }
    }

    #[test]
    fn cap_keeps_39_and_counts_rest() {
        let lines: Vec<String> = (0..52).map(|i| format!("Line {i}.")).collect();
        let out = cap(lines);
        assert_eq!(out.lines().count(), 40);
        assert_eq!(out.lines().last(), Some("And 13 more."));
        assert_eq!(cap(vec!["x".repeat(500)]).trim_end().chars().count(), 200);
    }

    #[test]
    fn gist_prefers_spec_and_strips_markdown() {
        let d = "Intro text.\n\n## Spec\n\n- [ ] **Make** the `thing` work. Then more. Third one.\n\n## Plan\n\nSECRET\n";
        assert_eq!(gist(d), "Make the thing work. Then more");
        assert_eq!(
            gist("Plain intro. Second. Third.\n## Plan\nx"),
            "Plain intro. Second"
        );
        assert_eq!(gist("## Plan\nx"), "");
    }

    #[test]
    fn issue_brief_facts_and_ages() {
        let mut i = iss(
            "K-1",
            "in-progress",
            3,
            "## Spec\n\nDo it.\n\n## Activity Log\n\n- 2026-10-05T10:00Z — did a thing\n",
        );
        i.priority = "none".into();
        let b = IssueBrief {
            issue: &i,
            milestone: Some("M1"),
            blockers: &[("K-2".into(), "backlog".into())],
            claimed_by: Some("bob"),
        };
        let out = issue(&b, now());
        assert!(out.contains("Status in-progress, milestone M1."), "{out}");
        assert!(out.contains("Spec: Do it."), "{out}");
        assert!(out.contains("Blocked by K-2 (backlog)."), "{out}");
        assert!(out.contains("Latest log 2h ago: did a thing."), "{out}");
        assert!(out.contains("Claimed by bob.") && out.contains("Last updated 3h ago."));
    }

    #[test]
    fn board_counts_lists_and_newest() {
        let v = vec![
            iss("K-1", "in-progress", 5, ""),
            iss("K-2", "backlog", 1, ""),
            iss("K-3", "backlog", 9, ""),
        ];
        let out = board("Project K, Kay.", &v, now());
        assert!(
            out.contains("3 issues: 1 in progress, 2 in backlog."),
            "{out}"
        );
        assert!(out.contains("In progress: K-1 Title K-1, updated 5h ago."));
        assert!(!out.contains("K-3"));
        assert!(out.contains("Newest change 1h ago: K-2 Title K-2, now backlog."));
        assert!(board("H.", &[], now()).contains("No issues."));
    }

    #[test]
    fn feed_orders_as_given() {
        let e = vec![(now(), "K-1".to_string(), "hello".to_string())];
        let out = feed("1d", &e, now());
        assert_eq!(out, "1 change since 1d.\njust now, K-1: hello.\n");
    }
}
