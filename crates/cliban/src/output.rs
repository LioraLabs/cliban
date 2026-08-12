//! JSON + table output builders.
//!
//! JSON keys are emitted in ALPHABETICAL order — a stable shape consumers can
//! sort-merge and diff. We rely on the workspace `serde_json` `preserve_order`
//! feature: insert keys in alphabetical order into a `Map` and the serialized
//! output preserves that order.
//!
use serde_json::{json, Map, Value};

/// The resolved output format for one command invocation.
///
/// Every command that emits a result consults this — never the raw `--json`
/// bool and never `is_terminal()` directly — so the whole binary agrees on
/// one contract: explicit flags beat `$CLIBAN_OUTPUT`, which beats
/// auto-detection (stdout a TTY → `Table`, piped/redirected → `Json`).
///
/// Mutations branch on it too: `Table` prints the one-line human
/// confirmation, `Json` echoes the mutated entity. `CLIBAN_OUTPUT=table`
/// with piped stdout therefore prints the human line — the env var pins the
/// *format*, not the terminal-ness.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Machine output: the exact shapes `--json` has always produced.
    Json,
    /// Human output: tables, listings, one-line confirmations.
    Table,
}

impl Mode {
    pub fn is_json(self) -> bool {
        self == Mode::Json
    }
}

/// Resolve the output mode from the two explicit flags, `$CLIBAN_OUTPUT`,
/// and finally whether stdout is a terminal.
///
/// `--json`/`--table` are mutually exclusive at the clap layer
/// (`conflicts_with`), so both being true cannot reach here. An unrecognized
/// `$CLIBAN_OUTPUT` value is ignored rather than fatal: the variable exists
/// to pin defaults for PTY-driven harnesses, and a typo'd pin failing every
/// command in the session would be worse than falling back to detection.
pub fn mode(json_flag: bool, table_flag: bool) -> Mode {
    if json_flag {
        return Mode::Json;
    }
    if table_flag {
        return Mode::Table;
    }
    if let Ok(v) = std::env::var("CLIBAN_OUTPUT") {
        match v.trim().to_ascii_lowercase().as_str() {
            "json" => return Mode::Json,
            "table" => return Mode::Table,
            _ => {}
        }
    }
    use std::io::IsTerminal;
    if std::io::stdout().is_terminal() {
        Mode::Table
    } else {
        Mode::Json
    }
}

/// Detail for a single-entity JSON read. Only auto-detected pipe output is
/// lean; an explicit flag or environment pin is a deliberate full read.
pub fn single_detail(json_flag: bool) -> Detail {
    if json_flag
        || std::env::var("CLIBAN_OUTPUT")
            .is_ok_and(|v| v.trim().eq_ignore_ascii_case("json"))
    {
        Detail::Full
    } else {
        Detail::Brief
    }
}

pub struct RelationOut {
    pub kind: String,
    pub target: String,
}

pub struct IssueJsonInputs {
    pub key: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: String,
    pub position: f64,
    pub archived: bool,
    /// Live claim, if any. Absent from the JSON when unclaimed, so boards that
    /// never claim keep their exact historical shape.
    pub claimed_by: Option<String>,
    pub due_date: Option<String>,
    pub completed_at: Option<String>,
    pub milestone: Option<String>,
    pub parent: Option<String>,
    pub labels: Vec<String>,
    pub relations: Vec<RelationOut>,
    pub created_at: String,
    pub updated_at: String,
}

fn num_pos(v: f64) -> Value {
    if v.fract() == 0.0 {
        json!(v as i64)
    } else {
        json!(v)
    }
}

fn opt_str(o: &Option<String>) -> Value {
    match o {
        Some(s) => json!(s),
        None => Value::Null,
    }
}

/// Whether a serialised row carries its `description` body.
///
/// A list command emits one row per issue and a body per row; on a real board
/// that is the whole payload. `cliban issue ls --project COOK --json` was
/// 2.27 MB, 95% of it descriptions that list consumers do not read — the
/// documented recipes all go `ls` to find keys, then `show` for detail. `ls`
/// and `show` shared one serialiser, so a list command emitted full bodies
/// with no way to decline them.
///
/// Every call site states which it wants, so a new list command cannot inherit
/// `Full` by forgetting to think about it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Detail {
    /// Every field except `description`. The default for query commands that
    /// return many rows.
    Brief,
    /// The Brief shape with one exception: `updated_at` keeps its stored
    /// microsecond precision, because a mutation echo is a valid CAS token —
    /// an agent chains `edit --if-updated-at` from the previous echo without
    /// a re-`show`.
    Echo,
    /// Every field. Single-entity reads (`show`, `current`) and `--full`.
    Full,
}

impl Detail {
    /// `Full` when the caller passed `--full`, else `Brief`.
    pub fn from_full_flag(full: bool) -> Self {
        if full {
            Self::Full
        } else {
            Self::Brief
        }
    }
}

/// Second-precision copy of a stored microsecond timestamp, for Brief rows.
/// `Full` output keeps the stored precision — `--if-updated-at` round-trips
/// through `show`, so the CAS token is never a trimmed value.
pub fn trim_usec(ts: &str) -> String {
    match (ts.find('.'), ts.ends_with('Z')) {
        (Some(dot), true) => format!("{}Z", &ts[..dot]),
        _ => ts.to_string(),
    }
}

/// `updated_at` for a non-Full row: trimmed in a list, stored-precision in a
/// mutation echo (the CAS-token exception — see [`Detail::Echo`]).
fn row_ts(ts: &str, detail: Detail) -> String {
    if detail == Detail::Echo {
        ts.to_string()
    } else {
        trim_usec(ts)
    }
}

pub fn build_issue_json(i: IssueJsonInputs, detail: Detail) -> Value {
    let mut m = Map::new();
    if detail != Detail::Full {
        // The list-row diet: a field that is null, empty, or the default is
        // absent, and per-row constants an agent never reads from a list
        // (`created_at`, `git_branch_name`, `position`) stay in `Full`.
        // Absent-means-default is already the row contract (`claimed_by`,
        // `completed_at`); Brief extends it to every optional field.
        if i.archived {
            m.insert("archived".into(), json!(true));
        }
        if let Some(c) = &i.claimed_by {
            m.insert("claimed_by".into(), json!(c));
        }
        if let Some(c) = &i.completed_at {
            m.insert("completed_at".into(), json!(trim_usec(c)));
        }
        if let Some(d) = &i.due_date {
            m.insert("due_date".into(), json!(d));
        }
        m.insert("key".into(), json!(i.key));
        if !i.labels.is_empty() {
            m.insert("labels".into(), json!(i.labels));
        }
        if let Some(ms) = &i.milestone {
            m.insert("milestone".into(), json!(ms));
        }
        if let Some(p) = &i.parent {
            m.insert("parent".into(), json!(p));
        }
        m.insert("priority".into(), json!(i.priority));
        if !i.relations.is_empty() {
            m.insert("relations".into(), relations_json(&i.relations));
        }
        m.insert("status".into(), json!(i.status));
        m.insert("title".into(), json!(i.title));
        m.insert("updated_at".into(), json!(row_ts(&i.updated_at, detail)));
        return Value::Object(m);
    }
    m.insert("archived".into(), json!(i.archived));
    if let Some(c) = &i.claimed_by {
        m.insert("claimed_by".into(), json!(c));
    }
    if let Some(c) = &i.completed_at {
        m.insert("completed_at".into(), json!(c));
    }
    m.insert("created_at".into(), json!(i.created_at));
    m.insert("description".into(), json!(i.description));
    m.insert("due_date".into(), opt_str(&i.due_date));
    m.insert(
        "git_branch_name".into(),
        json!(git_branch_name(&i.key, &i.title)),
    );
    m.insert("key".into(), json!(i.key));
    m.insert("labels".into(), json!(i.labels));
    m.insert("milestone".into(), opt_str(&i.milestone));
    m.insert("parent".into(), opt_str(&i.parent));
    m.insert("position".into(), num_pos(i.position));
    m.insert("priority".into(), json!(i.priority));
    m.insert("relations".into(), relations_json(&i.relations));
    m.insert("status".into(), json!(i.status));
    m.insert("title".into(), json!(i.title));
    m.insert("updated_at".into(), json!(i.updated_at));
    Value::Object(m)
}

fn relations_json(relations: &[RelationOut]) -> Value {
    let rels: Vec<Value> = relations
        .iter()
        .map(|r| {
            let mut rm = Map::new();
            rm.insert("type".into(), json!(r.kind));
            rm.insert("target".into(), json!(r.target));
            Value::Object(rm)
        })
        .collect();
    json!(rels)
}

/// Build the NDJSON object for a search match: the full issue JSON plus a
/// `score` field, keeping the keys alphabetical (`score` lands between
/// `relations` and `status`).
pub fn build_search_match_json(i: IssueJsonInputs, score: i64, detail: Detail) -> Value {
    let base = build_issue_json(i, detail);
    let mut entries: Vec<(String, Value)> = match base {
        Value::Object(m) => m.into_iter().collect(),
        _ => unreachable!("build_issue_json always returns an object"),
    };
    entries.push(("score".into(), json!(score)));
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    Value::Object(entries.into_iter().collect())
}

pub fn git_branch_name(key: &str, title: &str) -> String {
    let key_lower = key.to_lowercase();
    let mut slug = String::new();
    let mut prev_dash = false;
    for ch in title.to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            prev_dash = false;
        } else if !prev_dash {
            slug.push('-');
            prev_dash = true;
        }
    }
    let slug = slug.trim_matches('-');
    let mut slug = slug.to_string();
    if slug.is_empty() {
        return key_lower;
    }
    if slug.len() > 60 {
        slug.truncate(60);
        slug = slug.trim_end_matches('-').to_string();
    }
    format!("{key_lower}-{slug}")
}

/// Project JSON. Alpha keys (Full):
/// archived, auto_archive_done_after_days, created_at, description,
/// issue_seq, key, name, updated_at.
///
/// Brief drops the `description` body (a project's `## Notes` memory can be
/// tens of KB — `project ls` was 68 KB for 9 rows), plus `created_at`,
/// `issue_seq`, null policy, and `archived:false`.
#[allow(clippy::too_many_arguments)]
pub fn build_project_json(
    key: &str,
    name: &str,
    description: &str,
    archived: bool,
    auto_archive_done_after_days: Option<i64>,
    issue_seq: i64,
    created_at: &str,
    updated_at: &str,
    detail: Detail,
) -> Value {
    let mut m = Map::new();
    if detail != Detail::Full {
        if archived {
            m.insert("archived".into(), json!(true));
        }
        if let Some(d) = auto_archive_done_after_days {
            m.insert("auto_archive_done_after_days".into(), json!(d));
        }
        m.insert("key".into(), json!(key));
        m.insert("name".into(), json!(name));
        m.insert("updated_at".into(), json!(row_ts(updated_at, detail)));
        return Value::Object(m);
    }
    m.insert("archived".into(), json!(archived));
    m.insert(
        "auto_archive_done_after_days".into(),
        match auto_archive_done_after_days {
            Some(d) => json!(d),
            None => Value::Null,
        },
    );
    m.insert("created_at".into(), json!(created_at));
    m.insert("description".into(), json!(description));
    m.insert("issue_seq".into(), json!(issue_seq));
    m.insert("key".into(), json!(key));
    m.insert("name".into(), json!(name));
    m.insert("updated_at".into(), json!(updated_at));
    Value::Object(m)
}

/// Milestone JSON. Alpha keys:
/// created_at, description, issue_count, name, project, status,
/// target_date, updated_at.
#[allow(clippy::too_many_arguments)]
pub fn build_milestone_json(
    name: &str,
    project: Option<String>,
    description: &str,
    target_date: Option<String>,
    status: &str,
    created_at: &str,
    updated_at: &str,
    issue_count: i64,
    detail: Detail,
) -> Value {
    let mut m = Map::new();
    if detail != Detail::Full {
        m.insert("issue_count".into(), json!(issue_count));
        m.insert("name".into(), json!(name));
        if let Some(p) = &project {
            m.insert("project".into(), json!(p));
        }
        m.insert("status".into(), json!(status));
        if let Some(t) = &target_date {
            m.insert("target_date".into(), json!(t));
        }
        m.insert("updated_at".into(), json!(row_ts(updated_at, detail)));
        return Value::Object(m);
    }
    m.insert("created_at".into(), json!(created_at));
    m.insert("description".into(), json!(description));
    m.insert("issue_count".into(), json!(issue_count));
    m.insert("name".into(), json!(name));
    m.insert("project".into(), opt_str(&project));
    m.insert("status".into(), json!(status));
    m.insert("target_date".into(), opt_str(&target_date));
    m.insert("updated_at".into(), json!(updated_at));
    Value::Object(m)
}

// ---- Table writers ----

pub struct IssueRow {
    pub key: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    /// empty → "-"
    pub milestone: String,
    /// empty → "-"
    pub parent: String,
}

pub struct IssueSummaryRow {
    pub milestone: Option<String>,
    pub backlog: usize,
    pub in_progress: usize,
    pub blocked: usize,
    pub in_review: usize,
}

pub struct SearchRow {
    pub score: i64,
    pub key: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub milestone: String,
    pub parent: String,
}

fn dash(s: &str) -> String {
    if s.is_empty() {
        "-".into()
    } else {
        s.into()
    }
}

/// Render a grid of string cells (header + body): two spaces of padding
/// between columns, space-padded, nothing trailing.
///
/// Each column width = max cell width (in chars) across all rows. Every cell
/// except the LAST in its row is right-padded with spaces to (colwidth + 2);
/// the final cell in a row gets no trailing padding. Rows are newline
/// terminated.
fn render_tabwriter(rows: &[Vec<String>]) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let ncols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    // Column widths: max char count per column.
    let mut widths = vec![0usize; ncols];
    for r in rows {
        for (c, cell) in r.iter().enumerate() {
            let w = cell.chars().count();
            if w > widths[c] {
                widths[c] = w;
            }
        }
    }
    let padding = 2usize;
    let mut out = String::new();
    for r in rows {
        let last = r.len().saturating_sub(1);
        for (c, cell) in r.iter().enumerate() {
            out.push_str(cell);
            if c != last {
                let cellw = cell.chars().count();
                let target = widths[c] + padding;
                for _ in cellw..target {
                    out.push(' ');
                }
            }
        }
        out.push('\n');
    }
    out
}

pub fn write_issue_table(rows: &[IssueRow]) -> String {
    let mut grid: Vec<Vec<String>> = Vec::with_capacity(rows.len() + 1);
    grid.push(vec![
        "KEY".into(),
        "TITLE".into(),
        "STATUS".into(),
        "PRIORITY".into(),
        "MILESTONE".into(),
        "PARENT".into(),
    ]);
    for r in rows {
        grid.push(vec![
            r.key.clone(),
            r.title.clone(),
            r.status.clone(),
            r.priority.clone(),
            dash(&r.milestone),
            dash(&r.parent),
        ]);
    }
    render_tabwriter(&grid)
}

pub fn write_issue_summary_table(rows: &[IssueSummaryRow]) -> String {
    let mut grid = vec![vec![
        "MILESTONE".into(),
        "BACKLOG".into(),
        "IN-PROGRESS".into(),
        "BLOCKED".into(),
        "IN-REVIEW".into(),
    ]];
    grid.extend(rows.iter().map(|r| {
        vec![
            r.milestone.clone().unwrap_or_else(|| "-".into()),
            r.backlog.to_string(),
            r.in_progress.to_string(),
            r.blocked.to_string(),
            r.in_review.to_string(),
        ]
    }));
    render_tabwriter(&grid)
}

pub fn write_search_table(rows: &[SearchRow]) -> String {
    let mut grid: Vec<Vec<String>> = Vec::with_capacity(rows.len() + 1);
    grid.push(vec![
        "SCORE".into(),
        "KEY".into(),
        "TITLE".into(),
        "STATUS".into(),
        "PRIORITY".into(),
        "MILESTONE".into(),
        "PARENT".into(),
    ]);
    for r in rows {
        grid.push(vec![
            r.score.to_string(),
            r.key.clone(),
            r.title.clone(),
            r.status.clone(),
            r.priority.clone(),
            dash(&r.milestone),
            dash(&r.parent),
        ]);
    }
    render_tabwriter(&grid)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The flag layer of the resolver. The env/TTY layers are process-global,
    // so they are pinned end-to-end in tests/output_contract.rs instead of
    // here.
    #[test]
    fn explicit_flags_decide_the_mode_outright() {
        assert_eq!(mode(true, false), Mode::Json);
        assert_eq!(mode(false, true), Mode::Table);
    }

    /// The pre-`Detail` shape, for tests that assert the full key set.
    fn build_issue_json_full(i: IssueJsonInputs) -> Value {
        build_issue_json(i, Detail::Full)
    }

    #[test]
    fn issue_json_keys_alpha_and_position_is_integer() {
        let v = build_issue_json_full(IssueJsonInputs {
            key: "CLI-1".into(),
            title: "First".into(),
            description: "".into(),
            status: "backlog".into(),
            priority: "high".into(),
            claimed_by: None,
            position: 1000.0,
            archived: false,
            due_date: None,
            completed_at: None,
            milestone: None,
            parent: None,
            labels: vec!["bug".into()],
            relations: vec![RelationOut {
                kind: "blocked_by".into(),
                target: "CLI-2".into(),
            }],
            created_at: "2026-01-01T00:00:00.000000Z".into(),
            updated_at: "2026-01-01T00:00:00.000000Z".into(),
        });
        let s = serde_json::to_string(&v).unwrap();
        assert!(s.starts_with(r#"{"archived":false"#), "got {s}");
        assert!(
            s.contains(r#""position":1000"#) && !s.contains(r#""position":1000.0"#),
            "got {s}"
        );
        assert!(
            s.contains(r#""relations":[{"type":"blocked_by","target":"CLI-2"}]"#),
            "got {s}"
        );
        assert!(!s.contains("completed_at"), "got {s}");
        assert!(s.contains(r#""git_branch_name":"cli-1-first""#), "got {s}");
    }

    #[test]
    fn issue_json_completed_at_present_when_some() {
        let v = build_issue_json_full(IssueJsonInputs {
            key: "CLI-9".into(),
            title: "Done thing".into(),
            description: "d".into(),
            status: "done".into(),
            priority: "low".into(),
            claimed_by: None,
            position: 1500.5,
            archived: true,
            due_date: Some("2026-12-31".into()),
            completed_at: Some("2026-06-01T00:00:00.000000Z".into()),
            milestone: Some("M1".into()),
            parent: Some("CLI-1".into()),
            labels: vec![],
            relations: vec![],
            created_at: "2026-01-01T00:00:00.000000Z".into(),
            updated_at: "2026-01-01T00:00:00.000000Z".into(),
        });
        let s = serde_json::to_string(&v).unwrap();
        // completed_at sits between archived and created_at alphabetically.
        assert!(
            s.contains(
                r#""archived":true,"completed_at":"2026-06-01T00:00:00.000000Z","created_at""#
            ),
            "got {s}"
        );
        // Non-integral position keeps the float form.
        assert!(s.contains(r#""position":1500.5"#), "got {s}");
        assert!(
            s.contains(r#""git_branch_name":"cli-9-done-thing""#),
            "got {s}"
        );
    }

    #[test]
    fn git_branch_name_slug_rules() {
        assert_eq!(git_branch_name("CLI-1", "First issue"), "cli-1-first-issue");
        // leading/trailing non-alnum trimmed; runs collapse to single dash.
        assert_eq!(
            git_branch_name("ABC-2", "  Hello,  World!! "),
            "abc-2-hello-world"
        );
        // empty slug → just lowercased key.
        assert_eq!(git_branch_name("CLI-3", "!!!"), "cli-3");
        assert_eq!(git_branch_name("CLI-4", ""), "cli-4");
        // truncation to 60 chars then right-trim of dashes.
        let long = "a".repeat(70);
        let g = git_branch_name("CLI-5", &long);
        assert_eq!(g, format!("cli-5-{}", "a".repeat(60)));
    }

    #[test]
    fn project_json_alpha_keys() {
        let v = build_project_json(
            "CLI",
            "CLI Tool",
            "desc",
            false,
            Some(7),
            42,
            "2026-01-01T00:00:00.000000Z",
            "2026-01-02T00:00:00.000000Z",
            Detail::Full,
        );
        let s = serde_json::to_string(&v).unwrap();
        assert!(
            s.starts_with(r#"{"archived":false,"auto_archive_done_after_days":7,"created_at""#),
            "got {s}"
        );
        assert!(s.contains(r#""issue_seq":42"#), "got {s}");
        // ends with name then updated_at (alpha order: name < updated_at).
        assert!(s.contains(r#""name":"CLI Tool","updated_at""#), "got {s}");

        let v2 = build_project_json("X", "X", "", true, None, 0, "t", "t", Detail::Full);
        let s2 = serde_json::to_string(&v2).unwrap();
        assert!(
            s2.contains(r#""auto_archive_done_after_days":null"#),
            "got {s2}"
        );
    }

    #[test]
    fn brief_project_row_is_lean() {
        let v = build_project_json(
            "CLI",
            "CLI Tool",
            "a huge notes body",
            false,
            None,
            42,
            "2026-01-01T00:00:00.000000Z",
            "2026-01-02T00:00:00.123456Z",
            Detail::Brief,
        );
        let s = serde_json::to_string(&v).unwrap();
        assert_eq!(
            s,
            r#"{"key":"CLI","name":"CLI Tool","updated_at":"2026-01-02T00:00:00Z"}"#
        );
        // archived:true and a set policy survive the diet.
        let v2 = build_project_json("X", "X", "", true, Some(7), 0, "t", "tZ", Detail::Brief);
        let s2 = serde_json::to_string(&v2).unwrap();
        assert_eq!(
            s2,
            r#"{"archived":true,"auto_archive_done_after_days":7,"key":"X","name":"X","updated_at":"tZ"}"#
        );
    }

    #[test]
    fn brief_issue_row_omits_defaults_and_row_constants() {
        let v = build_issue_json(
            IssueJsonInputs {
                key: "CLI-1".into(),
                title: "First".into(),
                description: "body".into(),
                status: "backlog".into(),
                priority: "high".into(),
                claimed_by: None,
                position: 1000.0,
                archived: false,
                due_date: None,
                completed_at: None,
                milestone: None,
                parent: None,
                labels: vec![],
                relations: vec![],
                created_at: "2026-01-01T00:00:00.000000Z".into(),
                updated_at: "2026-01-01T00:00:00.654321Z".into(),
            },
            Detail::Brief,
        );
        let s = serde_json::to_string(&v).unwrap();
        assert_eq!(
            s,
            r#"{"key":"CLI-1","priority":"high","status":"backlog","title":"First","updated_at":"2026-01-01T00:00:00Z"}"#
        );
    }

    #[test]
    fn brief_issue_row_keeps_set_optionals() {
        let v = build_issue_json(
            IssueJsonInputs {
                key: "CLI-9".into(),
                title: "T".into(),
                description: "".into(),
                status: "done".into(),
                priority: "low".into(),
                claimed_by: Some("session:abc".into()),
                position: 1.0,
                archived: true,
                due_date: Some("2026-12-31".into()),
                completed_at: Some("2026-06-01T00:00:00.000009Z".into()),
                milestone: Some("M1".into()),
                parent: Some("CLI-1".into()),
                labels: vec!["bug".into()],
                relations: vec![RelationOut {
                    kind: "blocks".into(),
                    target: "CLI-2".into(),
                }],
                created_at: "t".into(),
                updated_at: "tZ".into(),
            },
            Detail::Brief,
        );
        let s = serde_json::to_string(&v).unwrap();
        assert_eq!(
            s,
            r#"{"archived":true,"claimed_by":"session:abc","completed_at":"2026-06-01T00:00:00Z","due_date":"2026-12-31","key":"CLI-9","labels":["bug"],"milestone":"M1","parent":"CLI-1","priority":"low","relations":[{"type":"blocks","target":"CLI-2"}],"status":"done","title":"T","updated_at":"tZ"}"#
        );
        // Brief never carries description / git_branch_name / position / created_at.
        assert!(!s.contains("description") && !s.contains("git_branch_name"));
        assert!(!s.contains("position") && !s.contains("created_at"));
    }

    #[test]
    fn trim_usec_only_touches_fractional_utc_stamps() {
        assert_eq!(
            trim_usec("2026-08-04T07:55:02.656951Z"),
            "2026-08-04T07:55:02Z"
        );
        assert_eq!(trim_usec("2026-08-04T07:55:02Z"), "2026-08-04T07:55:02Z");
        assert_eq!(trim_usec("2026-12-31"), "2026-12-31");
    }

    #[test]
    fn milestone_json_alpha_keys() {
        let v = build_milestone_json(
            "M1",
            Some("CLI".into()),
            "desc",
            Some("2026-12-31".into()),
            "active",
            "2026-01-01T00:00:00.000000Z",
            "2026-01-02T00:00:00.000000Z",
            5,
            Detail::Full,
        );
        let s = serde_json::to_string(&v).unwrap();
        assert!(s.starts_with(r#"{"created_at":"#), "got {s}");
        assert!(
            s.contains(r#""issue_count":5,"name":"M1","project":"CLI","status":"active","target_date":"2026-12-31","updated_at""#),
            "got {s}"
        );

        let v2 = build_milestone_json("M2", None, "", None, "planned", "t", "t", 0, Detail::Full);
        let s2 = serde_json::to_string(&v2).unwrap();
        assert!(s2.contains(r#""project":null"#), "got {s2}");
        assert!(s2.contains(r#""target_date":null"#), "got {s2}");
    }

    #[test]
    fn issue_table_matches_go_tabwriter() {
        let rows = vec![
            IssueRow {
                key: "CLI-1".into(),
                title: "Short".into(),
                status: "backlog".into(),
                priority: "none".into(),
                milestone: "".into(),
                parent: "".into(),
            },
            IssueRow {
                key: "CLI-2".into(),
                title: "A much longer title here".into(),
                status: "backlog".into(),
                priority: "none".into(),
                milestone: "".into(),
                parent: "".into(),
            },
        ];
        let got = write_issue_table(&rows);
        let want = "KEY    TITLE                     STATUS   PRIORITY  MILESTONE  PARENT\n\
                    CLI-1  Short                     backlog  none      -          -\n\
                    CLI-2  A much longer title here  backlog  none      -          -\n";
        assert_eq!(got, want);
    }

    #[test]
    fn search_table_has_leading_score_and_no_trailing_pad() {
        let rows = vec![SearchRow {
            score: 5580,
            key: "CLI-2".into(),
            title: "A much longer title here".into(),
            status: "backlog".into(),
            priority: "none".into(),
            milestone: "".into(),
            parent: "".into(),
        }];
        let got = write_search_table(&rows);
        let want = "SCORE  KEY    TITLE                     STATUS   PRIORITY  MILESTONE  PARENT\n\
                    5580   CLI-2  A much longer title here  backlog  none      -          -\n";
        assert_eq!(got, want);
    }
}
