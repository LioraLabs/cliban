//! One-shot migration: legacy Go SQLite (singular tables) -> cliban-core schema
//! (plural tables). Preserves all ids + issue seq (folded into key). See CLI-7.

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
