//! Port of `backend/lib/loom/issues.ex` + `Loom.Schema.Issue` changesets,
//! trimmed for cliban.
//!
//! The semantically-load-bearing pieces:
//!   * atomic `<KEY>-<N>` key generation (bump issue_seq in the txn),
//!   * default position (max+1000.0) / status (`backlog`) / priority (`none`),
//!   * a plain `move_issue` status flip that maintains `completed_at`
//!     (cliban has no terminal-state guard),
//!   * label set/add/remove,
//!   * the CLI `to_map` projection.

use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension};

use crate::contexts::{labels, milestones, projects};
use crate::error::{Error, Result};
use crate::projection::IssueMap;
use crate::rows;
use crate::schema::{Issue, DONE_STATUS, ISSUE_PRIORITIES, ISSUE_STATUSES};
use crate::time;

/// Attributes accepted by [`create`]. Mirrors the create attr map the CLI
/// layer assembles. `milestone` (a name) and `parent_key` are resolved here.
#[derive(Debug, Default, Clone)]
pub struct CreateIssue {
    pub title: String,
    pub description: Option<String>,
    pub status: Option<String>,
    pub priority: Option<String>,
    pub milestone: Option<String>,
    pub parent_key: Option<String>,
    pub due_date: Option<NaiveDate>,
    pub position: Option<f64>,
}

/// `create/2`. Runs entirely in one transaction (key bump + insert).
pub fn create(conn: &Connection, project_key: &str, attrs: CreateIssue) -> Result<Issue> {
    let tx = conn.unchecked_transaction()?;

    let project = projects::fetch_by_key(&tx, project_key)?;

    // resolve_milestone (name → id)
    let milestone_id = match &attrs.milestone {
        None => None,
        Some(name) => match milestones::get(&tx, &project.key, name)? {
            Some(m) => Some(m.id),
            None => {
                return Err(Error::validation(
                    "milestone",
                    &format!("not found: {name}"),
                ))
            }
        },
    };

    // resolve_parent (key → id)
    let parent_id = match &attrs.parent_key {
        None => None,
        Some(key) => match get_row_by_key(&tx, key)? {
            Some(i) => Some(i.id),
            None => {
                return Err(Error::validation(
                    "parent_key",
                    &format!("not found: {key}"),
                ))
            }
        },
    };

    let (_p, n) = projects::bump_issue_seq(&tx, &project.key)?;
    let key = format!("{}-{}", project.key, n);

    let status = attrs
        .status
        .clone()
        .unwrap_or_else(|| "backlog".to_string());
    let priority = attrs.priority.clone().unwrap_or_else(|| "none".to_string());

    let position = match attrs.position {
        Some(p) => p,
        None => default_position(&tx, project.id, &status)?,
    };

    let description = attrs.description.clone().unwrap_or_default();

    // --- validation (Issue.create_changeset) ---
    if attrs.title.is_empty() {
        return Err(Error::validation("title", "can't be blank"));
    }
    validate_inclusions(&status, &priority)?;
    validate_depth_limit(&tx, parent_id)?;

    // unique_constraint(:key) — key is generated, so this should never trip,
    // but mirror the changeset guard.
    if get_row_by_key(&tx, &key)?.is_some() {
        return Err(Error::validation("key", "has already been taken"));
    }

    let now = time::format_usec(time::now_usec());
    tx.execute(
        "INSERT INTO issues (key, project_id, milestone_id, parent_id, title, \
         description, status, priority, position, archived, due_date, \
         inserted_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, ?10, ?11, ?11)",
        params![
            key,
            project.id,
            milestone_id,
            parent_id,
            attrs.title,
            description,
            status,
            priority,
            position,
            attrs.due_date.map(time::format_date),
            now,
        ],
    )?;
    let id = tx.last_insert_rowid();
    let issue = get_row_by_id(&tx, id)?.expect("inserted");
    tx.commit()?;
    Ok(issue)
}

/// Options for [`list`]. `archived` defaults to false. `milestone` requires
/// `project` to be set (milestone names are project-scoped); otherwise the
/// result is empty (`where: false`).
#[derive(Debug, Default, Clone)]
pub struct ListOpts<'a> {
    pub project: Option<&'a str>,
    pub status: Option<&'a str>,
    pub milestone: Option<&'a str>,
    pub archived: bool,
}

pub fn list(conn: &Connection, opts: ListOpts) -> Result<Vec<Issue>> {
    let mut clauses = vec!["archived = ?1".to_string()];
    let mut binds: Vec<rusqlite::types::Value> = vec![(opts.archived as i64).into()];

    // project scope
    let project = match opts.project {
        None => None,
        Some(key) => match projects::get_by_key(conn, key)? {
            Some(p) => {
                clauses.push(format!("project_id = ?{}", binds.len() + 1));
                binds.push(p.id.into());
                Some(p)
            }
            None => return Ok(vec![]), // where: false
        },
    };

    if let Some(s) = opts.status {
        clauses.push(format!("status = ?{}", binds.len() + 1));
        binds.push(s.to_string().into());
    }

    if let Some(name) = opts.milestone {
        match project {
            None => return Ok(vec![]), // milestone without project → where: false
            Some(p) => match milestones::get(conn, &p.key, name)? {
                Some(m) => {
                    clauses.push(format!("milestone_id = ?{}", binds.len() + 1));
                    binds.push(m.id.into());
                }
                None => return Ok(vec![]),
            },
        }
    }

    let sql = format!(
        "SELECT {} FROM issues WHERE {} ORDER BY key ASC",
        rows::ISSUE_COLS,
        clauses.join(" AND ")
    );
    let mut stmt = conn.prepare(&sql)?;
    let out = stmt
        .query_map(rusqlite::params_from_iter(binds.iter()), rows::issue)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(out)
}

/// `get_by_key/1`.
pub fn get_by_key(conn: &Connection, key: &str) -> Result<Option<Issue>> {
    get_row_by_key(conn, key)
}

/// `get_by_id/1`: fetch an issue by primary key.
pub fn get_by_id(conn: &Connection, id: i64) -> Result<Option<Issue>> {
    get_row_by_id(conn, id)
}

/// Update attrs (mirror `Issue.update_changeset` cast list). `Some(None)` on
/// nullable fields clears them; `None` leaves untouched. A `status` here is
/// routed through [`move_issue`] by [`update`].
#[derive(Debug, Default, Clone)]
pub struct UpdateIssue {
    pub milestone_id: Option<Option<i64>>,
    pub parent_id: Option<Option<i64>>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub priority: Option<String>,
    pub position: Option<f64>,
    pub archived: Option<bool>,
    pub due_date: Option<Option<NaiveDate>>,
}

impl UpdateIssue {
    fn has_non_status(&self) -> bool {
        self.milestone_id.is_some()
            || self.parent_id.is_some()
            || self.title.is_some()
            || self.description.is_some()
            || self.priority.is_some()
            || self.position.is_some()
            || self.archived.is_some()
            || self.due_date.is_some()
    }
}

/// `update/2`. Routes a `status` change through [`move_issue`] (to maintain
/// `completed_at`) and non-status attrs through [`do_update`], both inside one
/// transaction.
pub fn update(conn: &Connection, issue: &Issue, attrs: UpdateIssue) -> Result<Issue> {
    let tx = conn.unchecked_transaction()?;
    let mut current = issue.clone();
    if let Some(status) = attrs.status.clone() {
        current = move_issue(&tx, &current, &status)?;
    }
    let mut rest = attrs;
    rest.status = None;
    if rest.has_non_status() {
        current = do_update(&tx, &current, rest)?;
    }
    tx.commit()?;
    Ok(current)
}

/// Plain status flip (cliban has no terminal-state guard). Sets `completed_at`
/// to now when moving to `done`, clears it otherwise. Repositions to the end of
/// the target column (max position + 1000) to mirror the Go `MoveIssue`.
pub fn move_issue(conn: &Connection, issue: &Issue, status: &str) -> Result<Issue> {
    if !ISSUE_STATUSES.contains(&status) {
        return Err(Error::validation("status", "is invalid"));
    }
    let now = time::format_usec(time::now_usec());
    let max: Option<f64> = conn.query_row(
        "SELECT max(position) FROM issues WHERE project_id = ?1 AND status = ?2",
        params![issue.project_id, status],
        |r| r.get(0),
    )?;
    let position = max.unwrap_or(0.0) + 1000.0;
    let completed_at: Option<String> = if status == DONE_STATUS {
        Some(now.clone())
    } else {
        None
    };
    conn.execute(
        "UPDATE issues SET status = ?1, position = ?2, completed_at = ?3, \
         updated_at = ?4 WHERE id = ?5",
        params![status, position, completed_at, now, issue.id],
    )?;
    Ok(get_row_by_id(conn, issue.id)?.expect("moved"))
}

fn do_update(conn: &Connection, issue: &Issue, attrs: UpdateIssue) -> Result<Issue> {
    // Build the effective row.
    let milestone_id = match attrs.milestone_id {
        Some(v) => v,
        None => issue.milestone_id,
    };
    let parent_id = match attrs.parent_id {
        Some(v) => v,
        None => issue.parent_id,
    };
    let title = attrs.title.clone().unwrap_or_else(|| issue.title.clone());
    let description = attrs
        .description
        .clone()
        .unwrap_or_else(|| issue.description.clone());
    let status = attrs.status.clone().unwrap_or_else(|| issue.status.clone());
    let priority = attrs
        .priority
        .clone()
        .unwrap_or_else(|| issue.priority.clone());
    let position = attrs.position.unwrap_or(issue.position);
    let archived = attrs.archived.unwrap_or(issue.archived);
    let due_date = match attrs.due_date {
        Some(v) => v,
        None => issue.due_date,
    };

    // validations from update_changeset
    validate_inclusions(&status, &priority)?;
    validate_depth_limit(conn, parent_id)?;

    let now = time::format_usec(time::now_usec());
    conn.execute(
        "UPDATE issues SET milestone_id = ?1, parent_id = ?2, title = ?3, \
         description = ?4, status = ?5, priority = ?6, position = ?7, \
         archived = ?8, due_date = ?9, updated_at = ?10 WHERE id = ?11",
        params![
            milestone_id,
            parent_id,
            title,
            description,
            status,
            priority,
            position,
            archived as i64,
            due_date.map(time::format_date),
            now,
            issue.id,
        ],
    )?;
    Ok(get_row_by_id(conn, issue.id)?.expect("updated"))
}

pub fn delete(conn: &Connection, issue: &Issue) -> Result<()> {
    conn.execute("DELETE FROM issues WHERE id = ?1", params![issue.id])?;
    Ok(())
}

// ---- labels ----

/// `set_labels/2`: replace the issue's labels with `names` (created on demand).
pub fn set_labels(conn: &Connection, issue: &Issue, names: &[String]) -> Result<Issue> {
    let project = projects::get_by_id(conn, issue.project_id)?.ok_or(Error::ProjectNotFound)?;
    let tx = conn.unchecked_transaction()?;
    let label_rows = labels::upsert_many(&tx, &project, names)?;

    tx.execute(
        "DELETE FROM issues_labels WHERE issue_id = ?1",
        params![issue.id],
    )?;
    for l in &label_rows {
        tx.execute(
            "INSERT OR IGNORE INTO issues_labels (issue_id, label_id) VALUES (?1, ?2)",
            params![issue.id, l.id],
        )?;
    }
    tx.commit()?;
    Ok(get_row_by_id(conn, issue.id)?.expect("issue"))
}

/// `add_label/2`: idempotent add.
pub fn add_label(conn: &Connection, issue: &Issue, name: &str) -> Result<Issue> {
    let mut names = label_names(conn, issue.id)?;
    if names.iter().any(|n| n == name) {
        return Ok(get_row_by_id(conn, issue.id)?.expect("issue"));
    }
    names.push(name.to_string());
    set_labels(conn, issue, &names)
}

/// `remove_label/2`: no-op if not present.
pub fn remove_label(conn: &Connection, issue: &Issue, name: &str) -> Result<Issue> {
    let names: Vec<String> = label_names(conn, issue.id)?
        .into_iter()
        .filter(|n| n != name)
        .collect();
    set_labels(conn, issue, &names)
}

// ---- projections ----

/// The CLI `to_map/1` projection for an issue.
pub fn to_map(conn: &Connection, issue: &Issue) -> Result<IssueMap> {
    let project = projects::get_by_id(conn, issue.project_id)?.map(|p| p.key);
    let milestone = match issue.milestone_id {
        Some(mid) => milestones::get_by_id(conn, mid)?.map(|m| m.name),
        None => None,
    };
    let parent = match issue.parent_id {
        Some(pid) => get_row_by_id(conn, pid)?.map(|i| i.key),
        None => None,
    };
    let labels = label_names(conn, issue.id)?;

    Ok(IssueMap {
        key: issue.key.clone(),
        title: issue.title.clone(),
        description: issue.description.clone(),
        status: issue.status.clone(),
        priority: issue.priority.clone(),
        position: issue.position,
        archived: issue.archived,
        due_date: issue.due_date,
        completed_at: issue.completed_at,
        project,
        milestone,
        labels,
        parent,
        created_at: issue.inserted_at,
        updated_at: issue.updated_at,
    })
}

// ---- internals ----

fn get_row_by_key(conn: &Connection, key: &str) -> Result<Option<Issue>> {
    let sql = format!("SELECT {} FROM issues WHERE key = ?1", rows::ISSUE_COLS);
    Ok(conn.query_row(&sql, params![key], rows::issue).optional()?)
}

fn get_row_by_id(conn: &Connection, id: i64) -> Result<Option<Issue>> {
    let sql = format!("SELECT {} FROM issues WHERE id = ?1", rows::ISSUE_COLS);
    Ok(conn.query_row(&sql, params![id], rows::issue).optional()?)
}

/// Label names for an issue, ordered by name (stable for the projections).
pub fn label_names(conn: &Connection, issue_id: i64) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT l.name FROM labels l \
         JOIN issues_labels il ON il.label_id = l.id \
         WHERE il.issue_id = ?1 ORDER BY l.name ASC",
    )?;
    let out = stmt
        .query_map(params![issue_id], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(out)
}

fn default_position(conn: &Connection, project_id: i64, status: &str) -> Result<f64> {
    // Mirror Go `CreateIssue`: end of the target STATUS column (per-status max),
    // not the project-wide max.
    let max: Option<f64> = conn.query_row(
        "SELECT max(position) FROM issues WHERE project_id = ?1 AND status = ?2",
        params![project_id, status],
        |r| r.get(0),
    )?;
    Ok(max.unwrap_or(0.0) + 1000.0)
}

fn validate_inclusions(status: &str, priority: &str) -> Result<()> {
    if !ISSUE_STATUSES.contains(&status) {
        return Err(Error::validation("status", "is invalid"));
    }
    if !ISSUE_PRIORITIES.contains(&priority) {
        return Err(Error::validation("priority", "is invalid"));
    }
    Ok(())
}

/// `validate_depth_limit/1`: parent must exist and itself have no parent
/// (max depth 2).
fn validate_depth_limit(conn: &Connection, parent_id: Option<i64>) -> Result<()> {
    match parent_id {
        None => Ok(()),
        Some(pid) => match get_row_by_id(conn, pid)? {
            None => Err(Error::validation("parent_id", "parent not found")),
            Some(p) if p.parent_id.is_none() => Ok(()),
            Some(_) => Err(Error::validation(
                "parent_id",
                "depth limit exceeded (max 2)",
            )),
        },
    }
}
