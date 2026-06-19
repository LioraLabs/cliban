//! Port of `backend/lib/loom/labels.ex` + `Loom.Schema.Label`.

use rusqlite::{params, Connection, OptionalExtension};

use crate::contexts::projects;
use crate::error::{Error, Result};
use crate::rows;
use crate::schema::{Label, Project};
use crate::time;

/// `create/2` — by project key + name.
pub fn create(conn: &Connection, project_key: &str, name: &str) -> Result<Label> {
    let p = match projects::get_by_key(conn, project_key)? {
        Some(p) => p,
        None => {
            return Err(Error::validation(
                "project",
                &format!("project not found: {project_key}"),
            ))
        }
    };
    insert(conn, &p, name)
}

fn insert(conn: &Connection, project: &Project, name: &str) -> Result<Label> {
    if name.is_empty() {
        return Err(Error::validation("name", "can't be blank"));
    }
    if get_row(conn, project.id, name)?.is_some() {
        return Err(Error::validation("name", "already exists in this project"));
    }
    let now = time::format_usec(time::now_usec());
    conn.execute(
        "INSERT INTO labels (project_id, name, inserted_at, updated_at) \
         VALUES (?1, ?2, ?3, ?3)",
        params![project.id, name, now],
    )?;
    let id = conn.last_insert_rowid();
    Ok(get_by_id(conn, id)?.expect("inserted"))
}

/// `list/1` — labels for a project key, ordered by name. Unknown project → [].
pub fn list(conn: &Connection, project_key: &str) -> Result<Vec<Label>> {
    match projects::get_by_key(conn, project_key)? {
        None => Ok(vec![]),
        Some(p) => {
            let sql = format!(
                "SELECT {} FROM labels WHERE project_id = ?1 ORDER BY name ASC",
                rows::LABEL_COLS
            );
            let mut stmt = conn.prepare(&sql)?;
            let out = stmt
                .query_map(params![p.id], rows::label)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(out)
        }
    }
}

/// `get/2` — by project key + name.
pub fn get(conn: &Connection, project_key: &str, name: &str) -> Result<Option<Label>> {
    match projects::get_by_key(conn, project_key)? {
        None => Ok(None),
        Some(p) => get_row(conn, p.id, name),
    }
}

/// `upsert_many/2`: resolve names to labels under `project`, creating any
/// missing. Preserves input order (mirrors `Enum.map`).
pub fn upsert_many(conn: &Connection, project: &Project, names: &[String]) -> Result<Vec<Label>> {
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        let label = match get_row(conn, project.id, name)? {
            Some(l) => l,
            None => insert(conn, project, name)?,
        };
        out.push(label);
    }
    Ok(out)
}

pub fn get_by_id(conn: &Connection, id: i64) -> Result<Option<Label>> {
    let sql = format!("SELECT {} FROM labels WHERE id = ?1", rows::LABEL_COLS);
    Ok(conn.query_row(&sql, params![id], rows::label).optional()?)
}

fn get_row(conn: &Connection, project_id: i64, name: &str) -> Result<Option<Label>> {
    let sql = format!(
        "SELECT {} FROM labels WHERE project_id = ?1 AND name = ?2",
        rows::LABEL_COLS
    );
    Ok(conn
        .query_row(&sql, params![project_id, name], rows::label)
        .optional()?)
}
