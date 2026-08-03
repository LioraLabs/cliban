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

/// Parse a "how far back" argument into an absolute instant. This is the one
/// place that decides what `--since` / `--updated-since` accept, because the
/// commonest agent mistake is guessing a form the CLI rejects:
///
/// - a duration counted back from `now`: `45s`, `90m`, `4h`, `3d`, `2w`
///   (fractions like `1.5h` are fine; `ns`/`us`/`ms` are accepted too)
/// - `today` / `yesterday` — UTC midnight boundaries, not rolling windows
/// - a bare date, `2026-07-25` → that day's UTC midnight
/// - a full RFC3339 timestamp
///
/// Everything is UTC, matching the timestamps cliban stores. Returns `None`
/// for anything unrecognized so callers can raise their own error.
pub fn parse_since(s: &str, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let s = s.trim();
    let midnight = |d: NaiveDate| d.and_hms_opt(0, 0, 0).map(|t| t.and_utc());
    match s.to_ascii_lowercase().as_str() {
        "" => return None,
        "now" => return Some(now),
        "today" => return midnight(now.date_naive()),
        "yesterday" => return midnight(now.date_naive().pred_opt()?),
        _ => {}
    }
    if let Some(d) = parse_duration(s) {
        return Some(now - d);
    }
    if let Some(d) = parse_date(s) {
        return midnight(d);
    }
    parse_ts(s)
}

/// A single signed decimal with a unit suffix: `s`/`m`/`h` plus `d` (day)
/// and `w` (week), which is what
/// people reach for first when asking "what changed since yesterday".
pub fn parse_duration(s: &str) -> Option<chrono::Duration> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let split = s.find(|c: char| c.is_ascii_alphabetic() || c == 'µ')?;
    let (num, unit) = s.split_at(split);
    let val: f64 = num.parse().ok()?;
    let secs = match unit {
        "ns" => val / 1_000_000_000.0,
        "us" | "µs" => val / 1_000_000.0,
        "ms" => val / 1_000.0,
        "s" => val,
        "m" => val * 60.0,
        "h" => val * 3600.0,
        "d" => val * 86_400.0,
        "w" => val * 604_800.0,
        _ => return None,
    };
    chrono::Duration::try_milliseconds((secs * 1000.0) as i64)
}

/// Compact human span between `then` and `now`, e.g. `just now`, `5m ago`,
/// `3h ago`, `2d ago`, `6w ago`, `1y ago`. Future timestamps read `in 3d`.
/// Units are approximate on purpose — this is a glanceable recency column,
/// not an audit trail.
pub fn relative(then: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let secs = (now - then).num_seconds();
    let (n, unit) = span(secs.abs());
    if n == 0 {
        return "just now".into();
    }
    if secs < 0 {
        format!("in {n}{unit}")
    } else {
        format!("{n}{unit} ago")
    }
}

fn span(secs: i64) -> (i64, &'static str) {
    const MIN: i64 = 60;
    const HOUR: i64 = 60 * MIN;
    const DAY: i64 = 24 * HOUR;
    const WEEK: i64 = 7 * DAY;
    const YEAR: i64 = 365 * DAY;
    match secs {
        s if s < MIN => (0, ""),
        s if s < HOUR => (s / MIN, "m"),
        s if s < DAY => (s / HOUR, "h"),
        s if s < WEEK => (s / DAY, "d"),
        s if s < YEAR => (s / WEEK, "w"),
        s => (s / YEAR, "y"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<Utc> {
        parse_ts(s).unwrap()
    }

    #[test]
    fn parse_since_accepts_every_form_an_agent_reaches_for() {
        let now = at("2026-07-26T12:00:00.000000Z");
        let cases = [
            ("4h", "2026-07-26T08:00:00.000000Z"),
            ("90m", "2026-07-26T10:30:00.000000Z"),
            ("1d", "2026-07-25T12:00:00.000000Z"),
            ("3d", "2026-07-23T12:00:00.000000Z"),
            ("2w", "2026-07-12T12:00:00.000000Z"),
            ("1.5h", "2026-07-26T10:30:00.000000Z"),
            ("today", "2026-07-26T00:00:00.000000Z"),
            ("yesterday", "2026-07-25T00:00:00.000000Z"),
            ("Yesterday", "2026-07-25T00:00:00.000000Z"),
            ("2026-07-20", "2026-07-20T00:00:00.000000Z"),
            ("2026-07-20T06:30:00Z", "2026-07-20T06:30:00.000000Z"),
            ("now", "2026-07-26T12:00:00.000000Z"),
        ];
        for (input, want) in cases {
            assert_eq!(parse_since(input, now), Some(at(want)), "for {input:?}");
        }
    }

    #[test]
    fn parse_since_rejects_junk() {
        let now = at("2026-07-26T12:00:00.000000Z");
        for bad in ["", "   ", "soon", "5", "1y2d", "last tuesday"] {
            assert_eq!(parse_since(bad, now), None, "for {bad:?}");
        }
    }

    #[test]
    fn relative_picks_the_coarsest_useful_unit() {
        let now = at("2026-07-26T12:00:00.000000Z");
        let cases = [
            ("2026-07-26T11:59:31.000000Z", "just now"),
            ("2026-07-26T11:55:00.000000Z", "5m ago"),
            ("2026-07-26T09:00:00.000000Z", "3h ago"),
            ("2026-07-24T12:00:00.000000Z", "2d ago"),
            ("2026-06-14T12:00:00.000000Z", "6w ago"),
            ("2024-07-26T12:00:00.000000Z", "2y ago"),
            ("2026-07-29T12:00:00.000000Z", "in 3d"),
        ];
        for (ts, want) in cases {
            assert_eq!(relative(at(ts), now), want, "for {ts}");
        }
    }
}
