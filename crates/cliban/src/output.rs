//! JSON + table output builders with Go-parity key ordering.
//!
//! The Go binary renders JSON from `map[string]any`, which `encoding/json`
//! serializes with ALPHABETICALLY sorted keys. We rely on the workspace
//! `serde_json` `preserve_order` feature: insert keys in alphabetical order
//! into a `Map` and the serialized output preserves that order.
//!
use serde_json::{json, Map, Value};

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

pub fn build_issue_json(i: IssueJsonInputs) -> Value {
    let mut m = Map::new();
    m.insert("archived".into(), json!(i.archived));
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
    let rels: Vec<Value> = i
        .relations
        .iter()
        .map(|r| {
            let mut rm = Map::new();
            rm.insert("type".into(), json!(r.kind));
            rm.insert("target".into(), json!(r.target));
            Value::Object(rm)
        })
        .collect();
    m.insert("relations".into(), json!(rels));
    m.insert("status".into(), json!(i.status));
    m.insert("title".into(), json!(i.title));
    m.insert("updated_at".into(), json!(i.updated_at));
    Value::Object(m)
}

/// Build the NDJSON object for a search match: the full issue JSON plus a
/// `score` field, with keys in alphabetical order (Go serializes a
/// `map[string]any` so adding `out["score"]` lands it alphabetically between
/// `relations` and `status`).
pub fn build_search_match_json(i: IssueJsonInputs, score: i64) -> Value {
    let base = build_issue_json(i);
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

/// Project JSON. Alpha keys:
/// archived, auto_archive_done_after_days, created_at, description,
/// issue_seq, key, name, updated_at.
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
) -> Value {
    let mut m = Map::new();
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
) -> Value {
    let mut m = Map::new();
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

// ---- Table writers (Go text/tabwriter parity) ----

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

/// Render a grid of string cells (header + body) using Go's text/tabwriter
/// semantics with minwidth=0, tabwidth=0, padding=2, padchar=' ', no flags.
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

    #[test]
    fn issue_json_keys_alpha_and_position_is_integer() {
        let v = build_issue_json(IssueJsonInputs {
            key: "CLI-1".into(),
            title: "First".into(),
            description: "".into(),
            status: "backlog".into(),
            priority: "high".into(),
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
        let v = build_issue_json(IssueJsonInputs {
            key: "CLI-9".into(),
            title: "Done thing".into(),
            description: "d".into(),
            status: "done".into(),
            priority: "low".into(),
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
            s.contains(r#""archived":true,"completed_at":"2026-06-01T00:00:00.000000Z","created_at""#),
            "got {s}"
        );
        // Non-integral position keeps the float form.
        assert!(s.contains(r#""position":1500.5"#), "got {s}");
        assert!(s.contains(r#""git_branch_name":"cli-9-done-thing""#), "got {s}");
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
        );
        let s = serde_json::to_string(&v).unwrap();
        assert!(
            s.starts_with(r#"{"archived":false,"auto_archive_done_after_days":7,"created_at""#),
            "got {s}"
        );
        assert!(s.contains(r#""issue_seq":42"#), "got {s}");
        // ends with name then updated_at (alpha order: name < updated_at).
        assert!(
            s.contains(r#""name":"CLI Tool","updated_at""#),
            "got {s}"
        );

        let v2 = build_project_json(
            "X",
            "X",
            "",
            true,
            None,
            0,
            "t",
            "t",
        );
        let s2 = serde_json::to_string(&v2).unwrap();
        assert!(
            s2.contains(r#""auto_archive_done_after_days":null"#),
            "got {s2}"
        );
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
        );
        let s = serde_json::to_string(&v).unwrap();
        assert!(
            s.starts_with(r#"{"created_at":"#),
            "got {s}"
        );
        assert!(
            s.contains(r#""issue_count":5,"name":"M1","project":"CLI","status":"active","target_date":"2026-12-31","updated_at""#),
            "got {s}"
        );

        let v2 = build_milestone_json("M2", None, "", None, "planned", "t", "t", 0);
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
