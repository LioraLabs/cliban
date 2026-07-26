//! Shared `--since` / `--updated-since` argument handling.
//!
//! One parser and one error message across every command, because "which time
//! formats does this flag take?" is a question the CLI should answer in the
//! failure itself rather than send the caller to `--help`.

use chrono::{DateTime, Utc};

use crate::errors::CliError;

/// The accepted forms, quoted verbatim in the error so a failed guess teaches
/// the right syntax.
pub const FORMS: &str =
    "want a duration (30m, 4h, 3d, 2w), 'today', 'yesterday', a date (2026-07-25), \
     or an RFC3339 timestamp";

/// Parse `s` relative to now. `flag` names the offending flag in the error.
pub fn parse(s: &str, flag: &str) -> Result<DateTime<Utc>, CliError> {
    cliban_core::time::parse_since(s, Utc::now())
        .ok_or_else(|| CliError::validation(format!("invalid {flag} {s:?} ({FORMS})")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_error_names_the_flag_and_lists_the_forms() {
        let err = parse("last tuesday", "--since").unwrap_err().message();
        assert!(err.contains("--since"), "{err}");
        assert!(err.contains("\"last tuesday\""), "{err}");
        assert!(err.contains("3d"), "the error teaches the syntax: {err}");
        assert!(err.contains("yesterday"), "{err}");
    }

    #[test]
    fn day_and_week_units_parse() {
        // The regression that started this: `1d` used to be rejected.
        assert!(parse("1d", "--since").is_ok());
        assert!(parse("2w", "--since").is_ok());
        assert!(parse("yesterday", "--since").is_ok());
        assert!(parse("2026-07-25", "--since").is_ok());
    }
}
