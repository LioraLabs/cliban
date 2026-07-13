//! One-shot migration: legacy Go SQLite (singular tables) -> cliban-core schema
//! (plural tables). Preserves all ids + issue seq (folded into key).

use std::path::Path;

use rusqlite::{params, Connection, OpenFlags};

/// Per-table row counts on the migrated target, for the round-trip report.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct MigrationReport {
    pub projects: i64,
    pub milestones: i64,
    pub issues: i64,
    pub labels: i64,
    pub issues_labels: i64,
    pub relations: i64,
}

/// Normalize a Go-written timestamp (nanosecond precision) to cliban-core's
/// microsecond `...Z` convention. Falls back to the raw string if unparseable.
fn norm_ts(s: &str) -> String {
    match cliban_core::time::parse_ts(s) {
        Some(dt) => cliban_core::time::format_usec(dt),
        None => s.to_string(),
    }
}

fn norm_opt_ts(s: Option<String>) -> Option<String> {
    s.map(|v| norm_ts(&v))
}

/// Migrate the legacy Go db at `from` into a fresh cliban-core db at `to`.
/// `to` must not already exist (we want a clean target). Returns target counts.
pub fn migrate(from: &Path, to: &Path) -> Result<MigrationReport, String> {
    if to.exists() {
        return Err(format!("target already exists: {}", to.display()));
    }
    let src = Connection::open_with_flags(from, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("open source {}: {e}", from.display()))?;
    let dst = Connection::open(to).map_err(|e| format!("open target {}: {e}", to.display()))?;
    cliban_core::migrations::run(&dst).map_err(|e| format!("schema: {e}"))?;
    dst.execute_batch("PRAGMA foreign_keys = OFF;")
        .map_err(|e| e.to_string())?;

    let tx = dst.unchecked_transaction().map_err(|e| e.to_string())?;
    copy_projects(&src, &tx)?;
    copy_milestones(&src, &tx)?;
    copy_labels(&src, &tx)?;
    copy_issues(&src, &tx)?;
    copy_issue_labels(&src, &tx)?;
    copy_relations(&src, &tx)?;
    tx.commit().map_err(|e| format!("commit: {e}"))?;

    report(&dst)
}

fn count(conn: &Connection, table: &str) -> Result<i64, String> {
    conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .map_err(|e| format!("count {table}: {e}"))
}

fn report(dst: &Connection) -> Result<MigrationReport, String> {
    Ok(MigrationReport {
        projects: count(dst, "projects")?,
        milestones: count(dst, "milestones")?,
        issues: count(dst, "issues")?,
        labels: count(dst, "labels")?,
        issues_labels: count(dst, "issues_labels")?,
        relations: count(dst, "issue_relation")?,
    })
}

fn copy_projects(src: &Connection, dst: &Connection) -> Result<(), String> {
    let mut stmt = src
        .prepare(
            "SELECT id, key, name, description, archived, issue_seq, \
                  auto_archive_done_after_days, created_at, updated_at FROM project",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, Option<i64>>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, String>(8)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (id, key, name, desc, archived, seq, auto, created, updated) =
            row.map_err(|e| e.to_string())?;
        dst.execute(
            "INSERT INTO projects (id, key, name, description, archived, \
             auto_archive_done_after_days, issue_seq, inserted_at, updated_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                id,
                key,
                name,
                desc,
                archived,
                auto,
                seq,
                norm_ts(&created),
                norm_ts(&updated)
            ],
        )
        .map_err(|e| format!("insert project {key}: {e}"))?;
    }
    Ok(())
}

fn copy_milestones(src: &Connection, dst: &Connection) -> Result<(), String> {
    let mut stmt = src
        .prepare(
            "SELECT id, project_id, name, description, target_date, status, \
                  created_at, updated_at FROM milestone",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (id, pid, name, desc, target, status, created, updated) =
            row.map_err(|e| e.to_string())?;
        dst.execute(
            "INSERT INTO milestones (id, project_id, name, description, target_date, \
             status, archived, inserted_at, updated_at) VALUES (?1,?2,?3,?4,?5,?6,0,?7,?8)",
            params![
                id,
                pid,
                name,
                desc,
                target,
                status,
                norm_ts(&created),
                norm_ts(&updated)
            ],
        )
        .map_err(|e| format!("insert milestone {id}: {e}"))?;
    }
    Ok(())
}

fn copy_labels(src: &Connection, dst: &Connection) -> Result<(), String> {
    let mut stmt = src
        .prepare("SELECT id, project_id, name, created_at FROM label")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (id, pid, name, created) = row.map_err(|e| e.to_string())?;
        let ts = norm_ts(&created);
        dst.execute(
            "INSERT INTO labels (id, project_id, name, inserted_at, updated_at) \
             VALUES (?1,?2,?3,?4,?4)",
            params![id, pid, name, ts],
        )
        .map_err(|e| format!("insert label {id}: {e}"))?;
    }
    Ok(())
}

fn project_keys(conn: &Connection) -> Result<std::collections::HashMap<i64, String>, String> {
    let mut stmt = conn
        .prepare("SELECT id, key FROM project")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?;
    let mut m = std::collections::HashMap::new();
    for row in rows {
        let (id, key) = row.map_err(|e| e.to_string())?;
        m.insert(id, key);
    }
    Ok(m)
}

fn copy_issues(src: &Connection, dst: &Connection) -> Result<(), String> {
    let pkeys = project_keys(src)?;
    let mut stmt = src
        .prepare(
            "SELECT id, project_id, milestone_id, parent_id, seq, title, description, \
                  status, priority, position, archived, due_date, completed_at, \
                  created_at, updated_at FROM issue",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, String>(8)?,
                r.get::<_, f64>(9)?,
                r.get::<_, i64>(10)?,
                r.get::<_, Option<String>>(11)?,
                r.get::<_, Option<String>>(12)?,
                r.get::<_, String>(13)?,
                r.get::<_, String>(14)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (
            id,
            pid,
            mid,
            parent,
            seq,
            title,
            desc,
            status,
            priority,
            position,
            archived,
            due,
            completed,
            created,
            updated,
        ) = row.map_err(|e| e.to_string())?;
        let pkey = pkeys
            .get(&pid)
            .ok_or_else(|| format!("issue {id}: unknown project_id {pid}"))?;
        let key = format!("{pkey}-{seq}");
        dst.execute(
            "INSERT INTO issues (id, key, project_id, milestone_id, parent_id, title, \
             description, status, priority, position, archived, due_date, completed_at, \
             inserted_at, updated_at) VALUES \
             (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
            params![
                id,
                key,
                pid,
                mid,
                parent,
                title,
                desc,
                status,
                priority,
                position,
                archived,
                due,
                norm_opt_ts(completed),
                norm_ts(&created),
                norm_ts(&updated)
            ],
        )
        .map_err(|e| format!("insert issue {key}: {e}"))?;
    }
    Ok(())
}

fn copy_issue_labels(src: &Connection, dst: &Connection) -> Result<(), String> {
    let mut stmt = src
        .prepare("SELECT issue_id, label_id FROM issue_label")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (iid, lid) = row.map_err(|e| e.to_string())?;
        dst.execute(
            "INSERT INTO issues_labels (issue_id, label_id) VALUES (?1,?2)",
            params![iid, lid],
        )
        .map_err(|e| format!("insert issue_label ({iid},{lid}): {e}"))?;
    }
    Ok(())
}

fn copy_relations(src: &Connection, dst: &Connection) -> Result<(), String> {
    let mut stmt = src
        .prepare("SELECT id, from_issue_id, to_issue_id, type, created_at FROM issue_relation")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (id, from, to, ty, created) = row.map_err(|e| e.to_string())?;
        dst.execute(
            "INSERT INTO issue_relation (id, from_issue_id, to_issue_id, type, created_at) \
             VALUES (?1,?2,?3,?4,?5)",
            params![id, from, to, ty, norm_ts(&created)],
        )
        .map_err(|e| format!("insert relation {id}: {e}"))?;
    }
    Ok(())
}
