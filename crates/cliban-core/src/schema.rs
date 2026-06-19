//! Row structs for cliban-core's tables. Plain data carriers loaded from
//! SQLite; validation lives in the contexts. Timestamps are `DateTime<Utc>`,
//! dates `NaiveDate`.

use chrono::{DateTime, NaiveDate, Utc};

#[derive(Debug, Clone)]
pub struct Project {
    pub id: i64,
    pub key: String,
    pub name: String,
    pub description: String,
    pub archived: bool,
    pub auto_archive_done_after_days: Option<i64>,
    pub issue_seq: i64,
    pub inserted_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct Milestone {
    pub id: i64,
    pub project_id: i64,
    pub name: String,
    pub description: String,
    pub target_date: Option<NaiveDate>,
    pub status: String,
    pub archived: bool,
    pub inserted_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct Label {
    pub id: i64,
    pub project_id: i64,
    pub name: String,
    pub inserted_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct Issue {
    pub id: i64,
    pub key: String,
    pub project_id: i64,
    pub milestone_id: Option<i64>,
    pub parent_id: Option<i64>,
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: String,
    pub position: f64,
    pub archived: bool,
    pub due_date: Option<NaiveDate>,
    pub completed_at: Option<DateTime<Utc>>,
    pub inserted_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ActivityLogEntry {
    pub id: i64,
    pub issue_id: i64,
    pub ts: DateTime<Utc>,
    pub kind: String,
    pub message: String,
    /// JSON-encoded string, exactly as stored.
    pub extra: String,
    pub inserted_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One outgoing relation edge between two issues.
#[derive(Debug, Clone)]
pub struct IssueRelation {
    pub id: i64,
    pub from_issue_id: i64,
    pub to_issue_id: i64,
    pub kind: String,
    pub created_at: DateTime<Utc>,
}

// ---- Enum vocabularies (cliban's, not loom's) ----

pub const ISSUE_STATUSES: &[&str] =
    &["backlog", "in-progress", "blocked", "in-review", "done"];

pub const ISSUE_PRIORITIES: &[&str] = &["none", "low", "medium", "high", "urgent"];

pub const MILESTONE_STATUSES: &[&str] = &["open", "completed", "cancelled"];

pub const RELATION_KINDS: &[&str] = &["blocks", "related_to"];

/// Status whose entry stamps `completed_at`.
pub const DONE_STATUS: &str = "done";
