//! Recording what the tool did, so the timeline is trustworthy without anyone
//! remembering to write it down.
//!
//! Agents are good at doing work and bad at narrating it. Every status move
//! and archive lands in `activity_log_entries` automatically; `cliban activity`
//! merges those with the prose an author wrote via `cliban issue log`. Set
//! `$CLIBAN_ACTOR` and each recorded entry is attributed, which is what makes
//! a shared board readable when several agents work it at once.

use rusqlite::Connection;
use serde_json::json;

use cliban_core::contexts::activity_log;
use cliban_core::schema::Issue;

/// Environment variable naming whoever is driving the CLI, e.g.
/// `CLIBAN_ACTOR=claude` or `CLIBAN_ACTOR=alex`.
pub const ACTOR_ENV: &str = "CLIBAN_ACTOR";

/// Who is acting, if they said so. Blank/whitespace counts as unset.
pub fn actor() -> Option<String> {
    std::env::var(ACTOR_ENV)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Record one audit entry against `issue`. Best-effort by design: a board
/// mutation that already succeeded must not be reported as failed because its
/// bookkeeping row didn't land.
pub fn record(conn: &Connection, issue: &Issue, kind: &str, message: &str) {
    let extra = match actor() {
        Some(a) => json!({ "actor": a }),
        None => json!({}),
    };
    let _ = activity_log::append(conn, issue, kind, message, &extra);
}

/// `status` entry for a move: `backlog → in-progress`, plus the caller's
/// `--note` when they gave a reason.
pub fn record_move(conn: &Connection, issue: &Issue, from: &str, to: &str, note: Option<&str>) {
    let mut message = format!("{from} → {to}");
    if let Some(note) = note.map(str::trim).filter(|n| !n.is_empty()) {
        message.push_str(": ");
        message.push_str(note);
    }
    record(conn, issue, "status", &message);
}

/// Describes what one `issue edit` changed, for the timeline. Row fields come
/// from a before/after comparison; labels and relations come from the flags,
/// since each of those operations either applied or aborted the command.
#[derive(Default)]
pub struct EditSummary {
    parts: Vec<String>,
}

impl EditSummary {
    /// `field: old → new`, skipped when the value didn't actually move (a
    /// no-op `--priority high` on an already-high issue is not an event).
    pub fn field(&mut self, name: &str, before: &str, after: &str) {
        if before == after {
            return;
        }
        self.parts.push(format!(
            "{name}: {} → {}",
            dash_if_blank(before),
            dash_if_blank(after)
        ));
    }

    /// Free text with no before/after, e.g. a description rewrite.
    pub fn note(&mut self, text: impl Into<String>) {
        self.parts.push(text.into());
    }

    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    pub fn message(&self) -> String {
        self.parts.join(", ")
    }
}

fn dash_if_blank(s: &str) -> &str {
    if s.is_empty() {
        "-"
    } else {
        s
    }
}

/// `issue log` writes its line into the description markdown *and* records it
/// here, so the note survives a later `--description` rewrite. Readers merge
/// the two sources and would show the entry twice, so they dedupe on this key:
/// the markdown line only carries minute precision, and the text is verbatim.
pub fn log_dedupe_key(ts: chrono::DateTime<chrono::Utc>, message: &str) -> (i64, String) {
    (ts.timestamp() / 60, message.trim().to_string())
}

/// Pull the actor back out of a stored `extra` blob for display.
pub fn actor_of(extra: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(extra)
        .ok()?
        .get("actor")?
        .as_str()
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actor_round_trips_through_the_extra_blob() {
        assert_eq!(actor_of(r#"{"actor":"claude"}"#).as_deref(), Some("claude"));
        assert_eq!(actor_of("{}"), None);
        assert_eq!(actor_of("not json"), None);
        assert_eq!(actor_of(r#"{"actor":7}"#), None);
    }
}
