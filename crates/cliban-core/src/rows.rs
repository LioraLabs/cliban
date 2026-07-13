//! Row to struct mappers. Column order here is the single source of truth for
//! the `SELECT <COLS>` strings in the contexts.

use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::Row;

use crate::schema::*;
use crate::time;

fn ts(row: &Row, idx: usize) -> rusqlite::Result<DateTime<Utc>> {
    let s: String = row.get(idx)?;
    time::parse_ts(&s).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            idx,
            rusqlite::types::Type::Text,
            format!("bad timestamp: {s:?}").into(),
        )
    })
}

fn opt_ts(row: &Row, idx: usize) -> rusqlite::Result<Option<DateTime<Utc>>> {
    let s: Option<String> = row.get(idx)?;
    Ok(s.and_then(|s| time::parse_ts(&s)))
}

fn opt_date(row: &Row, idx: usize) -> rusqlite::Result<Option<NaiveDate>> {
    let s: Option<String> = row.get(idx)?;
    Ok(s.and_then(|s| time::parse_date(&s)))
}

pub const PROJECT_COLS: &str = "id, key, name, description, archived, \
    auto_archive_done_after_days, issue_seq, inserted_at, updated_at";

pub fn project(row: &Row) -> rusqlite::Result<Project> {
    Ok(Project {
        id: row.get(0)?,
        key: row.get(1)?,
        name: row.get(2)?,
        description: row.get(3)?,
        archived: row.get(4)?,
        auto_archive_done_after_days: row.get(5)?,
        issue_seq: row.get(6)?,
        inserted_at: ts(row, 7)?,
        updated_at: ts(row, 8)?,
    })
}

pub const MILESTONE_COLS: &str = "id, project_id, name, description, \
    target_date, status, archived, inserted_at, updated_at";

pub fn milestone(row: &Row) -> rusqlite::Result<Milestone> {
    Ok(Milestone {
        id: row.get(0)?,
        project_id: row.get(1)?,
        name: row.get(2)?,
        description: row.get(3)?,
        target_date: opt_date(row, 4)?,
        status: row.get(5)?,
        archived: row.get(6)?,
        inserted_at: ts(row, 7)?,
        updated_at: ts(row, 8)?,
    })
}

pub const LABEL_COLS: &str = "id, project_id, name, inserted_at, updated_at";

pub fn label(row: &Row) -> rusqlite::Result<Label> {
    Ok(Label {
        id: row.get(0)?,
        project_id: row.get(1)?,
        name: row.get(2)?,
        inserted_at: ts(row, 3)?,
        updated_at: ts(row, 4)?,
    })
}

pub const ISSUE_COLS: &str = "id, key, project_id, milestone_id, parent_id, \
    title, description, status, priority, position, archived, due_date, \
    completed_at, inserted_at, updated_at";

pub fn issue(row: &Row) -> rusqlite::Result<Issue> {
    Ok(Issue {
        id: row.get(0)?,
        key: row.get(1)?,
        project_id: row.get(2)?,
        milestone_id: row.get(3)?,
        parent_id: row.get(4)?,
        title: row.get(5)?,
        description: row.get(6)?,
        status: row.get(7)?,
        priority: row.get(8)?,
        position: row.get(9)?,
        archived: row.get(10)?,
        due_date: opt_date(row, 11)?,
        completed_at: opt_ts(row, 12)?,
        inserted_at: ts(row, 13)?,
        updated_at: ts(row, 14)?,
    })
}

pub const ACTIVITY_COLS: &str = "id, issue_id, ts, kind, message, extra, \
    inserted_at, updated_at";

pub fn activity_log_entry(row: &Row) -> rusqlite::Result<ActivityLogEntry> {
    Ok(ActivityLogEntry {
        id: row.get(0)?,
        issue_id: row.get(1)?,
        ts: ts(row, 2)?,
        kind: row.get(3)?,
        message: row.get(4)?,
        extra: row.get(5)?,
        inserted_at: ts(row, 6)?,
        updated_at: ts(row, 7)?,
    })
}

pub const RELATION_COLS: &str = "id, from_issue_id, to_issue_id, type, created_at";

pub fn issue_relation(row: &Row) -> rusqlite::Result<IssueRelation> {
    Ok(IssueRelation {
        id: row.get(0)?,
        from_issue_id: row.get(1)?,
        to_issue_id: row.get(2)?,
        kind: row.get(3)?,
        created_at: ts(row, 4)?,
    })
}
