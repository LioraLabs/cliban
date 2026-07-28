//! Where the activity mailbox remembers what you've already read.
//!
//! One tiny state file per database — an RFC3339 timestamp, nothing else —
//! kept client-side under `$XDG_STATE_HOME/cliban` (not in the shared DB,
//! because "read" is a per-person fact: on a shared board your teammate's
//! glance at the mailbox must not clear yours). Best-effort by design: a
//! missing or unwritable state dir costs the badge, never the session.
//!
//! `CLIBAN_TUI_SEEN_FILE` overrides the path outright (tests, screenshots).

use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

/// The state file for `db`: env override, else
/// `$XDG_STATE_HOME/cliban/seen-<hash>` (fallback `~/.local/state`). The
/// hash only namespaces databases; a hasher change just resets the badge.
pub fn path_for(db: &Path) -> PathBuf {
    if let Ok(p) = std::env::var("CLIBAN_TUI_SEEN_FILE") {
        if !p.trim().is_empty() {
            return PathBuf::from(p);
        }
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    db.hash(&mut h);
    let dir = match std::env::var("XDG_STATE_HOME") {
        Ok(v) if !v.is_empty() => PathBuf::from(v),
        _ => cliban_core::paths::home_dir().join(".local/state"),
    };
    dir.join("cliban").join(format!("seen-{:016x}", h.finish()))
}

pub fn load(path: &Path) -> Option<DateTime<Utc>> {
    let s = std::fs::read_to_string(path).ok()?;
    DateTime::parse_from_rfc3339(s.trim())
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

/// Best-effort write; the badge is bookkeeping, not board state.
pub fn store(path: &Path, ts: DateTime<Utc>) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, ts.to_rfc3339());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_the_state_file() {
        let dir = std::env::temp_dir().join(format!("cliban-seen-test-{}", std::process::id()));
        let file = dir.join("seen");
        let ts = Utc::now();
        store(&file, ts);
        let back = load(&file).expect("stored timestamp should load");
        // RFC3339 keeps sub-second precision, so this is exact.
        assert_eq!(back, ts);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_or_garbled_files_read_as_never_seen() {
        assert_eq!(load(Path::new("/nonexistent/cliban-seen")), None);
        let dir = std::env::temp_dir().join(format!("cliban-seen-bad-{}", std::process::id()));
        let file = dir.join("seen");
        if let Some(d) = file.parent() {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(&file, "not a timestamp").unwrap();
        assert_eq!(load(&file), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn different_databases_get_different_files() {
        // (Only when the env override is not set — don't set it here.)
        let a = path_for(Path::new("/tmp/a.db"));
        let b = path_for(Path::new("/tmp/b.db"));
        assert_ne!(a, b);
        assert!(a.to_string_lossy().contains("cliban"));
    }
}
