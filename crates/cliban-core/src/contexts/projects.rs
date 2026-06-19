//! Port of `backend/lib/loom/projects.ex` + `Loom.Schema.Project` changesets.
//!
//! All functions take `&Connection` and run on the store's writer thread.

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{Error, Result};
use crate::rows;
use crate::schema::Project;
use crate::time;

/// `Project.@key_re`: `^[A-Z][A-Z0-9]{1,9}$` (2-10 chars, leading letter).
fn valid_key(key: &str) -> bool {
    let bytes = key.as_bytes();
    if bytes.len() < 2 || bytes.len() > 10 {
        return false;
    }
    if !bytes[0].is_ascii_uppercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

/// Attributes accepted by [`create`]. Mirrors `Project.create_changeset` cast
/// list. `key` is upcased; `description` defaults to "".
#[derive(Debug, Default, Clone)]
pub struct CreateProject {
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub auto_archive_done_after_days: Option<i64>,
}

pub fn create(conn: &Connection, attrs: CreateProject) -> Result<Project> {
    let key = attrs.key.to_uppercase();

    if attrs.name.is_empty() {
        return Err(Error::validation("name", "can't be blank"));
    }
    if !valid_key(&key) {
        return Err(Error::validation(
            "key",
            "must be 2-10 chars, uppercase letters/digits, starting with a letter",
        ));
    }

    // unique_constraint(:key)
    if get_by_key(conn, &key)?.is_some() {
        return Err(Error::validation("key", "has already been taken"));
    }

    let now = time::format_usec(time::now_usec());
    let desc = attrs.description.unwrap_or_default();
    conn.execute(
        "INSERT INTO projects (key, name, description, archived, \
         auto_archive_done_after_days, issue_seq, inserted_at, updated_at) \
         VALUES (?1, ?2, ?3, 0, ?4, 0, ?5, ?5)",
        params![key, attrs.name, desc, attrs.auto_archive_done_after_days, now],
    )?;
    let id = conn.last_insert_rowid();
    Ok(get_by_id(conn, id)?.expect("just inserted"))
}

pub fn list(conn: &Connection) -> Result<Vec<Project>> {
    let sql = format!(
        "SELECT {} FROM projects ORDER BY key ASC",
        rows::PROJECT_COLS
    );
    let mut stmt = conn.prepare(&sql)?;
    let out = stmt
        .query_map([], rows::project)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(out)
}

/// `get_by_key/1` — upcases the key (matches the Elixir `String.upcase`).
pub fn get_by_key(conn: &Connection, key: &str) -> Result<Option<Project>> {
    let sql = format!(
        "SELECT {} FROM projects WHERE key = ?1",
        rows::PROJECT_COLS
    );
    Ok(conn
        .query_row(&sql, params![key.to_uppercase()], rows::project)
        .optional()?)
}

pub fn get_by_id(conn: &Connection, id: i64) -> Result<Option<Project>> {
    let sql = format!("SELECT {} FROM projects WHERE id = ?1", rows::PROJECT_COLS);
    Ok(conn.query_row(&sql, params![id], rows::project).optional()?)
}

/// `fetch_by_key/1` — returns ProjectNotFound instead of None.
pub fn fetch_by_key(conn: &Connection, key: &str) -> Result<Project> {
    get_by_key(conn, key)?.ok_or(Error::ProjectNotFound)
}

/// Attributes for [`update`]. Mirrors `Project.update_changeset` cast list.
#[derive(Debug, Default, Clone)]
pub struct UpdateProject {
    pub name: Option<String>,
    pub description: Option<String>,
    pub archived: Option<bool>,
    pub auto_archive_done_after_days: Option<Option<i64>>,
}

pub fn update(conn: &Connection, p: &Project, attrs: UpdateProject) -> Result<Project> {
    // validate_required([:name]) — when name is being set, it must be non-empty.
    if let Some(name) = &attrs.name {
        if name.is_empty() {
            return Err(Error::validation("name", "can't be blank"));
        }
    }

    let name = attrs.name.clone().unwrap_or_else(|| p.name.clone());
    let description = attrs
        .description
        .clone()
        .unwrap_or_else(|| p.description.clone());
    let archived = attrs.archived.unwrap_or(p.archived);
    let auto = match attrs.auto_archive_done_after_days {
        Some(v) => v,
        None => p.auto_archive_done_after_days,
    };
    let now = time::format_usec(time::now_usec());

    conn.execute(
        "UPDATE projects SET name = ?1, description = ?2, archived = ?3, \
         auto_archive_done_after_days = ?4, updated_at = ?5 WHERE id = ?6",
        params![name, description, archived as i64, auto, now, p.id],
    )?;
    Ok(get_by_id(conn, p.id)?.expect("updated"))
}

pub fn delete(conn: &Connection, p: &Project) -> Result<()> {
    conn.execute("DELETE FROM projects WHERE id = ?1", params![p.id])?;
    Ok(())
}

/// `bump_issue_seq!/1`: atomically increment and return the new value. The
/// caller (issues::create) runs inside a transaction already; this just does
/// the UPDATE ... RETURNING-equivalent. Returns `(updated_project, next)`.
pub fn bump_issue_seq(conn: &Connection, key: &str) -> Result<(Project, i64)> {
    let p = fetch_by_key(conn, key)?;
    let next = p.issue_seq + 1;
    let now = time::format_usec(time::now_usec());
    conn.execute(
        "UPDATE projects SET issue_seq = ?1, updated_at = ?2 WHERE id = ?3",
        params![next, now, p.id],
    )?;
    let updated = get_by_id(conn, p.id)?.expect("bumped");
    Ok((updated, next))
}
