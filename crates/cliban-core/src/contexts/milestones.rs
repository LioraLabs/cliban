//! Port of `backend/lib/loom/milestones.ex` + `Loom.Schema.Milestone`.

use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension};

use crate::contexts::projects;
use crate::error::{Error, Result};
use crate::rows;
use crate::schema::{Milestone, MILESTONE_STATUSES};
use crate::time;

#[derive(Debug, Clone)]
pub struct CreateMilestone {
    /// Project key (resolved to project_id). Mirrors the `%{project: key}`
    /// create clause.
    pub project: String,
    pub name: String,
    pub description: Option<String>,
    pub target_date: Option<NaiveDate>,
    pub status: Option<String>,
}

pub fn create(conn: &Connection, attrs: CreateMilestone) -> Result<Milestone> {
    let project = match projects::get_by_key(conn, &attrs.project)? {
        Some(p) => p,
        None => {
            return Err(Error::validation(
                "project",
                &format!("project not found: {}", attrs.project),
            ))
        }
    };

    if attrs.name.is_empty() {
        return Err(Error::validation("name", "can't be blank"));
    }
    let status = attrs.status.clone().unwrap_or_else(|| "open".to_string());
    if !MILESTONE_STATUSES.contains(&status.as_str()) {
        return Err(Error::validation("status", "is invalid"));
    }
    // unique_constraint([:project_id, :name])
    if get_row(conn, project.id, &attrs.name)?.is_some() {
        return Err(Error::validation("name", "already exists in this project"));
    }

    let now = time::format_usec(time::now_usec());
    let desc = attrs.description.unwrap_or_default();
    let target = attrs.target_date.map(time::format_date);
    conn.execute(
        "INSERT INTO milestones (project_id, name, description, target_date, \
         status, archived, inserted_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?6)",
        params![project.id, attrs.name, desc, target, status, now],
    )?;
    let id = conn.last_insert_rowid();
    Ok(get_by_id(conn, id)?.expect("inserted"))
}

#[derive(Debug, Default, Clone)]
pub struct UpdateMilestone {
    pub name: Option<String>,
    pub description: Option<String>,
    pub target_date: Option<Option<NaiveDate>>,
    pub status: Option<String>,
    pub archived: Option<bool>,
}

pub fn update(conn: &Connection, m: &Milestone, attrs: UpdateMilestone) -> Result<Milestone> {
    if let Some(name) = &attrs.name {
        if name.is_empty() {
            return Err(Error::validation("name", "can't be blank"));
        }
    }
    if let Some(status) = &attrs.status {
        if !MILESTONE_STATUSES.contains(&status.as_str()) {
            return Err(Error::validation("status", "is invalid"));
        }
    }
    let name = attrs.name.clone().unwrap_or_else(|| m.name.clone());
    // unique within project, if name changed
    if name != m.name {
        if let Some(existing) = get_row(conn, m.project_id, &name)? {
            if existing.id != m.id {
                return Err(Error::validation("name", "already exists in this project"));
            }
        }
    }
    let description = attrs
        .description
        .clone()
        .unwrap_or_else(|| m.description.clone());
    let target = match attrs.target_date {
        Some(v) => v,
        None => m.target_date,
    };
    let status = attrs.status.clone().unwrap_or_else(|| m.status.clone());
    let archived = attrs.archived.unwrap_or(m.archived);
    let now = time::format_usec(time::now_usec());

    conn.execute(
        "UPDATE milestones SET name = ?1, description = ?2, target_date = ?3, \
         status = ?4, archived = ?5, updated_at = ?6 WHERE id = ?7",
        params![
            name,
            description,
            target.map(time::format_date),
            status,
            archived as i64,
            now,
            m.id
        ],
    )?;
    Ok(get_by_id(conn, m.id)?.expect("updated"))
}

/// `list/1` — ordered by name; optionally scoped to a project key. An unknown
/// project key yields an empty list (mirrors `where: false`).
pub fn list(conn: &Connection, project: Option<&str>) -> Result<Vec<Milestone>> {
    let (where_clause, project_id) = match project {
        None => (String::new(), None),
        Some(key) => match projects::get_by_key(conn, key)? {
            Some(p) => ("WHERE project_id = ?1".to_string(), Some(p.id)),
            None => return Ok(vec![]),
        },
    };
    let sql = format!(
        "SELECT {} FROM milestones {} ORDER BY name ASC",
        rows::MILESTONE_COLS,
        where_clause
    );
    let mut stmt = conn.prepare(&sql)?;
    let out = match project_id {
        Some(pid) => stmt
            .query_map(params![pid], rows::milestone)?
            .collect::<rusqlite::Result<Vec<_>>>()?,
        None => stmt
            .query_map([], rows::milestone)?
            .collect::<rusqlite::Result<Vec<_>>>()?,
    };
    Ok(out)
}

/// `get/2` — by project key + name.
pub fn get(conn: &Connection, project_key: &str, name: &str) -> Result<Option<Milestone>> {
    match projects::get_by_key(conn, project_key)? {
        None => Ok(None),
        Some(p) => get_row(conn, p.id, name),
    }
}

pub fn get_by_id(conn: &Connection, id: i64) -> Result<Option<Milestone>> {
    let sql = format!(
        "SELECT {} FROM milestones WHERE id = ?1",
        rows::MILESTONE_COLS
    );
    Ok(conn
        .query_row(&sql, params![id], rows::milestone)
        .optional()?)
}

fn get_row(conn: &Connection, project_id: i64, name: &str) -> Result<Option<Milestone>> {
    let sql = format!(
        "SELECT {} FROM milestones WHERE project_id = ?1 AND name = ?2",
        rows::MILESTONE_COLS
    );
    Ok(conn
        .query_row(&sql, params![project_id, name], rows::milestone)
        .optional()?)
}
