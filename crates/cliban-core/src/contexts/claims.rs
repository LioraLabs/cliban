//! Who is holding an issue right now. A claim is ownership for the duration of
//! a work session, not a status: `in-progress` says work has started,
//! `claimed_by` says which actor may move it. The distinction matters once
//! several agent sessions share one board — the frontier query treats a claimed
//! issue as taken even before its first status move lands.
//!
//! Storage is the `issue_claims` side table (`migrations::CLAIMS_DDL`), created
//! lazily by [`ensure`] rather than by a schema-version bump, because the
//! migration ledger is shared with sibling forks — see the essay on
//! `REMOTE_LINKS_DDL` in `migrations.rs`.

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{Error, Result};
use crate::migrations::CLAIMS_DDL;
use crate::schema::Issue;
use crate::time;

/// A live claim on an issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    pub claimed_by: String,
    pub claimed_at: String,
}

/// Guarantee the `issue_claims` table exists. Called before every claims read
/// or write; this — not the migration runner — is what provides the table on a
/// database whose ledger a newer sibling owns.
pub fn ensure(conn: &Connection) -> Result<()> {
    for stmt in CLAIMS_DDL {
        conn.execute_batch(stmt)?;
    }
    Ok(())
}

/// The claim on `issue_id`, if any.
pub fn get(conn: &Connection, issue_id: i64) -> Result<Option<Claim>> {
    ensure(conn)?;
    Ok(conn
        .query_row(
            "SELECT claimed_by, claimed_at FROM issue_claims WHERE issue_id = ?1",
            params![issue_id],
            |r| {
                Ok(Claim {
                    claimed_by: r.get(0)?,
                    claimed_at: r.get(1)?,
                })
            },
        )
        .optional()?)
}

/// Claim `issue` for `by`. Re-claiming your own issue is an idempotent no-op;
/// claiming over another actor's live claim fails unless `force`.
pub fn claim(conn: &Connection, issue: &Issue, by: &str, force: bool) -> Result<Claim> {
    let by = by.trim();
    if by.is_empty() {
        return Err(Error::validation("claimed_by", "can't be blank"));
    }
    if let Some(existing) = get(conn, issue.id)? {
        if existing.claimed_by == by {
            return Ok(existing);
        }
        if !force {
            return Err(Error::validation(
                "claim",
                &format!(
                    "{} is already claimed by {} (pass --force to take it over)",
                    issue.key, existing.claimed_by
                ),
            ));
        }
    }
    let now = time::format_usec(time::now_usec());
    conn.execute(
        "INSERT INTO issue_claims (issue_id, claimed_by, claimed_at) VALUES (?1, ?2, ?3) \
         ON CONFLICT (issue_id) DO UPDATE SET claimed_by = ?2, claimed_at = ?3",
        params![issue.id, by, now],
    )?;
    Ok(Claim {
        claimed_by: by.to_string(),
        claimed_at: now,
    })
}

/// Release the claim on `issue`. Returns the actor who held it, or `None` when
/// it wasn't claimed (releasing an unclaimed issue is not an error — the end
/// state is what was asked for).
pub fn release(conn: &Connection, issue: &Issue) -> Result<Option<String>> {
    let existing = get(conn, issue.id)?;
    conn.execute(
        "DELETE FROM issue_claims WHERE issue_id = ?1",
        params![issue.id],
    )?;
    Ok(existing.map(|c| c.claimed_by))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contexts::{issues, projects};

    fn setup() -> (Connection, Issue) {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        crate::migrations::run(&conn).unwrap();
        projects::create(
            &conn,
            projects::CreateProject {
                key: "ACME".into(),
                name: "Acme".into(),
                description: None,
                auto_archive_done_after_days: None,
            },
        )
        .unwrap();
        let issue = issues::create(
            &conn,
            "ACME",
            issues::CreateIssue {
                title: "t".into(),
                ..Default::default()
            },
        )
        .unwrap();
        (conn, issue)
    }

    #[test]
    fn claim_release_round_trip() {
        let (conn, issue) = setup();
        assert!(get(&conn, issue.id).unwrap().is_none());
        claim(&conn, &issue, "session:abc", false).unwrap();
        assert_eq!(
            get(&conn, issue.id).unwrap().unwrap().claimed_by,
            "session:abc"
        );
        assert_eq!(
            release(&conn, &issue).unwrap().as_deref(),
            Some("session:abc")
        );
        assert!(get(&conn, issue.id).unwrap().is_none());
    }

    #[test]
    fn reclaim_by_same_actor_is_idempotent() {
        let (conn, issue) = setup();
        claim(&conn, &issue, "a", false).unwrap();
        claim(&conn, &issue, "a", false).unwrap();
    }

    #[test]
    fn claim_over_another_actor_requires_force() {
        let (conn, issue) = setup();
        claim(&conn, &issue, "a", false).unwrap();
        assert!(claim(&conn, &issue, "b", false).is_err());
        claim(&conn, &issue, "b", true).unwrap();
        assert_eq!(get(&conn, issue.id).unwrap().unwrap().claimed_by, "b");
    }

    #[test]
    fn releasing_unclaimed_is_a_quiet_no_op() {
        let (conn, issue) = setup();
        assert_eq!(release(&conn, &issue).unwrap(), None);
    }

    #[test]
    fn ensure_creates_the_table_on_a_ledgerless_db() {
        // Simulates a database whose ledger a newer sibling owns: run() would
        // decline to migrate, so ensure() must provide the table on its own.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE issues (id INTEGER PRIMARY KEY)")
            .unwrap();
        ensure(&conn).unwrap();
        conn.execute("INSERT INTO issues (id) VALUES (1)", [])
            .unwrap();
        conn.execute(
            "INSERT INTO issue_claims (issue_id, claimed_by, claimed_at) VALUES (1, 'x', 'now')",
            [],
        )
        .unwrap();
    }
}
