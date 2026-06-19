//! Timestamp + date formatting helpers matching Ecto's SQLite TEXT encoding.
//!
//! Ecto stores `:utc_datetime_usec` columns as ISO8601 strings with
//! microsecond precision and a `Z` suffix, e.g. `2026-05-23T23:39:36.809441Z`
//! (verified against the live `loom.db`). `:date` columns store as
//! `YYYY-MM-DD`. We reproduce these exactly so a Rust-written row is
//! byte-indistinguishable from a BEAM-written one and round-trips through the
//! same DB.

use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};

/// Current UTC time, truncated to microseconds (Ecto `:utc_datetime_usec`
/// resolution). `DateTime<Utc>` carries nanos; SQLite TEXT only keeps 6
/// fractional digits, so we truncate the in-memory value too to keep the
/// round-trip exact.
pub fn now_usec() -> DateTime<Utc> {
    truncate_usec(Utc::now())
}

/// Truncate a `DateTime<Utc>` to microsecond precision.
pub fn truncate_usec(dt: DateTime<Utc>) -> DateTime<Utc> {
    let nanos = dt.timestamp_subsec_nanos();
    let usec = nanos - (nanos % 1000);
    // Rebuild from the same second + truncated sub-second.
    dt.with_nanosecond(usec).unwrap_or(dt)
}

use chrono::Timelike;

/// Format a UTC timestamp the way Ecto's `:utc_datetime_usec` does:
/// always 6 fractional digits, `Z` suffix. Matches `DateTime.to_iso8601/1`
/// on a microsecond-precision datetime.
pub fn format_usec(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(SecondsFormat::Micros, true)
}

/// Parse a timestamp stored by Ecto. Accepts the canonical 6-digit-fraction
/// `Z` form plus, defensively, any RFC3339 variant (some legacy rows from
/// `DateTime.truncate(:second)` carry no fraction).
pub fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

/// Format a date as `YYYY-MM-DD` (Ecto `:date`).
pub fn format_date(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

/// Parse a `YYYY-MM-DD` date string.
pub fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}
