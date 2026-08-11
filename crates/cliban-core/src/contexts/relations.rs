//! Issue-to-issue edges:
//! `blocks` (directional) and `related_to` (symmetric). `blocked_by` is the
//! read-side reverse of an incoming `blocks` edge.

use rusqlite::{params, Connection};

use crate::error::{Error, Result};
use crate::rows;
use crate::schema::{Issue, RELATION_KINDS};
use crate::time;

/// One outgoing edge with the other issue's key. `kind` is one of
/// `blocks` / `related_to` / `blocked_by` (the last only appears in reads).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relation {
    pub kind: String,
    pub target_key: String,
}

/// Resolve an issue id by `<KEY>` (e.g. `CLI-5`).
fn issue_id_by_key(conn: &Connection, key: &str) -> Result<i64> {
    conn.query_row("SELECT id FROM issues WHERE key = ?1", params![key], |r| {
        r.get(0)
    })
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Error::NamedNotFound(key.to_uppercase()),
        other => Error::Sqlite(other),
    })
}

fn insert_edge(conn: &Connection, from: i64, to: i64, kind: &str, now: &str) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO issue_relation (from_issue_id, to_issue_id, type, created_at) \
         VALUES (?1, ?2, ?3, ?4)",
        params![from, to, kind, now],
    )?;
    Ok(())
}

/// `add/3`: create a relation from `from_key` to `to_key`. `related_to` also
/// inserts the symmetric reverse edge.
pub fn add(conn: &Connection, from_key: &str, to_key: &str, kind: &str) -> Result<()> {
    if !RELATION_KINDS.contains(&kind) {
        return Err(Error::validation("type", "invalid relation kind"));
    }
    let from = issue_id_by_key(conn, from_key)?;
    let to = issue_id_by_key(conn, to_key)?;
    if from == to {
        return Err(Error::validation("to", "issue cannot relate to itself"));
    }
    let now = time::format_usec(time::now_usec());
    let tx = conn.unchecked_transaction()?;
    insert_edge(&tx, from, to, kind, &now)?;
    if kind == "related_to" {
        insert_edge(&tx, to, from, kind, &now)?;
    }
    tx.commit()?;
    Ok(())
}

/// `remove/3`: delete a relation (and the symmetric reverse for `related_to`).
pub fn remove(conn: &Connection, from_key: &str, to_key: &str, kind: &str) -> Result<()> {
    let from = issue_id_by_key(conn, from_key)?;
    let to = issue_id_by_key(conn, to_key)?;
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM issue_relation WHERE from_issue_id = ?1 AND to_issue_id = ?2 AND type = ?3",
        params![from, to, kind],
    )?;
    if kind == "related_to" {
        tx.execute(
            "DELETE FROM issue_relation WHERE from_issue_id = ?1 AND to_issue_id = ?2 AND type = ?3",
            params![to, from, kind],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// `for_issue/1`: outgoing edges + incoming `blocks` edges surfaced as
/// `blocked_by`. Sorted by (kind, target key) for stability.
pub fn for_issue(conn: &Connection, issue_id: i64) -> Result<Vec<Relation>> {
    let mut out = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT r.type, t.key FROM issue_relation r \
             JOIN issues t ON t.id = r.to_issue_id \
             WHERE r.from_issue_id = ?1 ORDER BY r.type, t.key",
        )?;
        let rows = stmt.query_map(params![issue_id], |r| {
            Ok(Relation {
                kind: r.get(0)?,
                target_key: r.get(1)?,
            })
        })?;
        for row in rows {
            out.push(row?);
        }
    }
    {
        let mut stmt = conn.prepare(
            "SELECT f.key FROM issue_relation r \
             JOIN issues f ON f.id = r.from_issue_id \
             WHERE r.to_issue_id = ?1 AND r.type = 'blocks' ORDER BY f.key",
        )?;
        let rows = stmt.query_map(params![issue_id], |r| {
            Ok(Relation {
                kind: "blocked_by".to_string(),
                target_key: r.get(0)?,
            })
        })?;
        for row in rows {
            out.push(row?);
        }
    }
    Ok(out)
}

/// The frontier: takeable issues — `backlog` status, not archived, no open
/// blocker, and no live claim. The complement of [`list_blocked`], and the
/// query behind `cliban issue ls --ready`. `milestone` is project-scoped like
/// `issues::list`: given without `project_key`, the result is empty.
pub fn list_ready(
    conn: &Connection,
    project_key: Option<&str>,
    parent_key: Option<&str>,
    milestone: Option<&str>,
) -> Result<Vec<Issue>> {
    crate::contexts::claims::ensure(conn)?;
    let mut clauses: Vec<String> = Vec::new();
    let mut binds: Vec<rusqlite::types::Value> = Vec::new();
    if let Some(key) = project_key {
        clauses.push(format!(
            "i.project_id = (SELECT id FROM projects WHERE key = ?{})",
            binds.len() + 1
        ));
        binds.push(key.to_string().into());
    }
    if let Some(key) = parent_key {
        clauses.push(format!(
            "i.parent_id = (SELECT id FROM issues WHERE key = ?{})",
            binds.len() + 1
        ));
        binds.push(key.to_string().into());
    }
    if let Some(name) = milestone {
        let Some(project) = project_key else {
            return Ok(vec![]);
        };
        match crate::contexts::milestones::get(conn, project, name)? {
            Some(m) => {
                clauses.push(format!("i.milestone_id = ?{}", binds.len() + 1));
                binds.push(m.id.into());
            }
            None => return Ok(vec![]),
        }
    }
    let extra = if clauses.is_empty() {
        String::new()
    } else {
        format!(" AND {}", clauses.join(" AND "))
    };
    let sql = format!(
        "SELECT {} FROM issues i \
         WHERE i.archived = 0 AND i.status = 'backlog' \
         AND NOT EXISTS (SELECT 1 FROM issue_relation r \
             JOIN issues b ON b.id = r.from_issue_id \
             WHERE r.to_issue_id = i.id AND r.type = 'blocks' \
             AND b.archived = 0 AND b.status != 'done') \
         AND NOT EXISTS (SELECT 1 FROM issue_claims c WHERE c.issue_id = i.id)\
         {extra} ORDER BY i.status, i.position",
        rows::ISSUE_COLS
            .split(", ")
            .map(|c| format!("i.{c}"))
            .collect::<Vec<_>>()
            .join(", "),
    );
    let mut stmt = conn.prepare(&sql)?;
    let out = stmt
        .query_map(rusqlite::params_from_iter(binds.iter()), rows::issue)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(out)
}

/// `list_blocked/1`: non-archived issues with at least one open (non-done,
/// non-archived) blocker. `project_key = None` spans all projects.
pub fn list_blocked(conn: &Connection, project_key: Option<&str>) -> Result<Vec<Issue>> {
    let cols = rows::ISSUE_COLS
        .split(", ")
        .map(|c| format!("i.{c}"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut sql = format!(
        "SELECT DISTINCT {cols} FROM issues i \
         JOIN issue_relation r ON r.to_issue_id = i.id AND r.type = 'blocks' \
         JOIN issues blocker ON blocker.id = r.from_issue_id \
         WHERE i.archived = 0 AND blocker.archived = 0 AND blocker.status != 'done'"
    );
    let out = match project_key {
        Some(key) => {
            sql.push_str(
                " AND i.project_id = (SELECT id FROM projects WHERE key = ?1) \
                 ORDER BY i.status, i.position",
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt
                .query_map(params![key], rows::issue)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        }
        None => {
            sql.push_str(" ORDER BY i.status, i.position");
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt
                .query_map([], rows::issue)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        }
    };
    Ok(out)
}
