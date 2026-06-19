//! Port of `backend/lib/loom/activity_log.ex`.
//!
//! Append-only per-issue log + the `## Activity Log` markdown mirror that
//! every append rewrites into `issues.description` (spec §6.7). The mirror
//! logic is reproduced without a regex engine: the Elixir regex
//! `^## Activity Log[ \t]*\n(?:.*?)(?=^## |\z)` (multiline, dot-all) just means
//! "from the `## Activity Log` header line up to the next `## ` header at
//! column 0, or end of string". We implement that as a line scan.

use rusqlite::{params, Connection};

use crate::error::Result;
use crate::rows;
use crate::schema::{ActivityLogEntry, Issue};
use crate::time;

const SECTION_HEADER: &str = "## Activity Log";

/// `append/4`. `extra` is a serde_json value, JSON-encoded into the row
/// exactly like `Jason.encode!(extra)`. On success, mirrors the full log into
/// the issue's description (same transaction).
pub fn append(
    conn: &Connection,
    issue: &Issue,
    kind: &str,
    message: &str,
    extra: &serde_json::Value,
) -> Result<ActivityLogEntry> {
    let extra_str = serde_json::to_string(extra)?;
    let now = time::now_usec();
    let now_str = time::format_usec(now);

    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO activity_log_entries (issue_id, ts, kind, message, extra, \
         inserted_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        params![issue.id, now_str, kind, message, extra_str, now_str],
    )?;
    let id = tx.last_insert_rowid();
    let entry = {
        let sql = format!(
            "SELECT {} FROM activity_log_entries WHERE id = ?1",
            rows::ACTIVITY_COLS
        );
        tx.query_row(&sql, params![id], rows::activity_log_entry)?
    };

    mirror_into_description(&tx, issue.id)?;
    tx.commit()?;
    Ok(entry)
}

/// `list_for_issue/2` — ascending by ts, default limit 200.
pub fn list_for_issue(
    conn: &Connection,
    issue_id: i64,
    limit: i64,
) -> Result<Vec<ActivityLogEntry>> {
    let sql = format!(
        "SELECT {} FROM activity_log_entries WHERE issue_id = ?1 \
         ORDER BY ts ASC LIMIT ?2",
        rows::ACTIVITY_COLS
    );
    let mut stmt = conn.prepare(&sql)?;
    let out = stmt
        .query_map(params![issue_id, limit], rows::activity_log_entry)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(out)
}

/// `render/1` — one markdown line per entry. Format mirrors the Elixir
/// `render_line/1`: `<iso8601>  <kind padded to 8>  <msg>`, trailing
/// whitespace trimmed per line.
pub fn render(entries: &[ActivityLogEntry]) -> String {
    entries
        .iter()
        .map(render_line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_line(e: &ActivityLogEntry) -> String {
    let kind_padded = format!("{:<8}", e.kind);
    let line = format!("{}  {}  {}", time::format_usec(e.ts), kind_padded, e.message);
    line.trim_end().to_string()
}

/// `merge_activity_log_section/2` (the pure helper). Public for tests + the
/// mirror path.
pub fn merge_activity_log_section(description: Option<&str>, body: &str) -> String {
    let section = build_section(body);
    match description {
        None => section,
        Some("") => section,
        Some(desc) if has_activity_section(desc) => replace_section(desc, &section),
        Some(desc) => append_section(desc, &section),
    }
}

// ---- internals ----

fn mirror_into_description(conn: &Connection, issue_id: i64) -> Result<()> {
    let entries = list_for_issue(conn, issue_id, 200)?;
    let body = render(&entries);

    // Reload description to pick up concurrent edits.
    let current: String = conn.query_row(
        "SELECT description FROM issues WHERE id = ?1",
        params![issue_id],
        |r| r.get(0),
    )?;

    let new_desc = merge_activity_log_section(Some(&current), &body);
    if new_desc != current {
        let now = time::format_usec(time::now_usec());
        conn.execute(
            "UPDATE issues SET description = ?1, updated_at = ?2 WHERE id = ?3",
            params![new_desc, now, issue_id],
        )?;
    }
    Ok(())
}

fn build_section(body: &str) -> String {
    let trimmed = body.trim_end();
    if trimmed.is_empty() {
        format!("{SECTION_HEADER}\n")
    } else {
        format!("{SECTION_HEADER}\n\n{trimmed}\n")
    }
}

fn has_activity_section(description: &str) -> bool {
    find_section_start(description).is_some()
}

/// Byte offset of the start of a line that is exactly `## Activity Log`
/// (optionally followed by trailing spaces/tabs), at column 0.
fn find_section_start(description: &str) -> Option<usize> {
    let mut offset = 0usize;
    for line in description.split_inclusive('\n') {
        let trimmed_nl = line.strip_suffix('\n').unwrap_or(line);
        if is_activity_header_line(trimmed_nl) {
            return Some(offset);
        }
        offset += line.len();
    }
    None
}

/// A `## Activity Log` header line: the literal header, then only spaces/tabs.
fn is_activity_header_line(line: &str) -> bool {
    if let Some(rest) = line.strip_prefix(SECTION_HEADER) {
        rest.chars().all(|c| c == ' ' || c == '\t')
    } else {
        false
    }
}

/// Replace the existing `## Activity Log` section with `section`. The section
/// runs from its header up to (but not including) the next `## ` header at
/// column 0, or end of string. Mirrors the Elixir `replace_section/2` which
/// emits `section <> "\n"` then normalizes trailing blank lines.
fn replace_section(description: &str, section: &str) -> String {
    let start = find_section_start(description).expect("has section");
    // Find the end: the next line (after the header) that begins a new
    // `## ` header at column 0.
    let after_header = &description[start..];
    let mut end_rel = after_header.len();
    let mut scanned = 0usize;
    let mut first = true;
    for line in after_header.split_inclusive('\n') {
        if first {
            // skip the header line itself
            first = false;
            scanned += line.len();
            continue;
        }
        let content = line.strip_suffix('\n').unwrap_or(line);
        if content.starts_with("## ") {
            end_rel = scanned;
            break;
        }
        scanned += line.len();
    }
    let end = start + end_rel;

    let mut out = String::with_capacity(description.len() + section.len());
    out.push_str(&description[..start]);
    out.push_str(section);
    out.push('\n');
    out.push_str(&description[end..]);
    normalize_trailing_blank_lines(&out)
}

fn normalize_trailing_blank_lines(s: &str) -> String {
    format!("{}\n", s.trim_end())
}

fn append_section(description: &str, section: &str) -> String {
    let base = description.trim_end();
    format!("{base}\n\n{section}")
}
