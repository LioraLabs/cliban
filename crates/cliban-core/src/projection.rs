//! JSON projection structs — the CLI `to_map` shapes the core serializes.
//!
//! Field ORDER here matches the source map literals; with serde +
//! `preserve_order` the emitted JSON key order follows struct declaration
//! order, which keeps diffs against the future parity harness minimal. Dates
//! and timestamps serialize as the same strings Ecto/Jason produce (see
//! [`crate::time`]); `serialize_with` keeps that exact.

use serde::Serialize;

use crate::time;

mod ser {
    //! Custom serializers so chrono types emit Ecto/Jason-compatible strings.
    use super::time;
    use chrono::{DateTime, NaiveDate, Utc};
    use serde::Serializer;

    pub fn ts<S: Serializer>(dt: &DateTime<Utc>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&time::format_usec(*dt))
    }

    pub fn opt_ts<S: Serializer>(dt: &Option<DateTime<Utc>>, s: S) -> Result<S::Ok, S::Error> {
        match dt {
            Some(d) => s.serialize_str(&time::format_usec(*d)),
            None => s.serialize_none(),
        }
    }

    pub fn opt_date<S: Serializer>(d: &Option<NaiveDate>, s: S) -> Result<S::Ok, S::Error> {
        match d {
            Some(d) => s.serialize_str(&time::format_date(*d)),
            None => s.serialize_none(),
        }
    }
}

/// The CLI `to_map/1` projection for an issue. Keys (in order):
/// `key, title, description, status, priority, position, archived, due_date,
/// completed_at, project, milestone, labels, parent, created_at, updated_at`.
#[derive(Debug, Clone, Serialize)]
pub struct IssueMap {
    pub key: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: String,
    pub position: f64,
    pub archived: bool,
    #[serde(serialize_with = "ser::opt_date")]
    pub due_date: Option<chrono::NaiveDate>,
    #[serde(serialize_with = "ser::opt_ts")]
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub project: Option<String>,
    pub milestone: Option<String>,
    pub labels: Vec<String>,
    pub parent: Option<String>,
    #[serde(serialize_with = "ser::ts")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(serialize_with = "ser::ts")]
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// The CLI `to_map/1` projection for a project. Keys (in order):
/// `key, name, description, archived, auto_archive_done_after_days, issue_seq,
/// created_at, updated_at`.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectMap {
    pub key: String,
    pub name: String,
    pub description: String,
    pub archived: bool,
    pub auto_archive_done_after_days: Option<i64>,
    pub issue_seq: i64,
    #[serde(serialize_with = "ser::ts")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(serialize_with = "ser::ts")]
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// The CLI `to_map/1` projection for a milestone. Keys (in order):
/// `name, project, description, target_date, status, created_at, updated_at`.
#[derive(Debug, Clone, Serialize)]
pub struct MilestoneMap {
    pub name: String,
    pub project: Option<String>,
    pub description: String,
    #[serde(serialize_with = "ser::opt_date")]
    pub target_date: Option<chrono::NaiveDate>,
    pub status: String,
    #[serde(serialize_with = "ser::ts")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(serialize_with = "ser::ts")]
    pub updated_at: chrono::DateTime<chrono::Utc>,
}
