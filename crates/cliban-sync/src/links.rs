//! `remote_links` — which local issue is which remote issue.
//!
//! # Why this creates its own table
//!
//! [`ensure_table`] runs `CREATE TABLE IF NOT EXISTS` at the top of every sync
//! command rather than trusting the migration in `cliban_core::migrations`.
//! That looks redundant, and on a cliban-only database it is. It is not
//! redundant on a *shared* one.
//!
//! cliban's sibling tools vendor a fork of cliban-core and write the same
//! default database file. `migrations::run` handles that by noticing a ledger
//! version newer than its own and returning without migrating — using the
//! database as it stands is correct there, because those forks only ever add
//! tables. But it means that on a machine where the sibling's schema is ahead,
//! cliban's own new migrations never run, and `remote_links` would silently
//! never exist. Creating it here, idempotently, is what actually guarantees
//! the table; the migration is for fresh installs and tidiness.

use chrono::{DateTime, Utc};
use cliban_core::time;
use rusqlite::{params, Connection, Row};

use cliban_core::{Error, Result};

/// The DDL. Defined in `cliban_core::migrations` — the migration and this
/// module apply the same statements, and a second copy here would be a schema
/// waiting to drift.
pub use cliban_core::migrations::REMOTE_LINKS_DDL as DDL;

const COLS: &str = "id, provider, entity, local_id, remote_id, remote_key, \
                    remote_updated_at, base_hash, last_synced_at";

/// One local↔remote pairing.
#[derive(Debug, Clone)]
pub struct RemoteLink {
    pub id: i64,
    pub provider: String,
    pub entity: String,
    pub local_id: i64,
    /// The remote's stable identifier — for Linear, a UUID. This is what we
    /// address on write; `remote_key` is for humans and can change when an
    /// issue moves team.
    pub remote_id: String,
    /// The remote's human key, e.g. `ENG-412`.
    pub remote_key: String,
    /// The remote's `updatedAt` as of the last successful sync. The stale-write
    /// guard compares against this.
    pub remote_updated_at: Option<DateTime<Utc>>,
    /// Hash of the remote-owned fields as of the last sync, so a refresh can
    /// tell "you edited a field Linear owns" from "nothing changed locally".
    pub base_hash: Option<String>,
    pub last_synced_at: DateTime<Utc>,
}

/// The fields a caller supplies; the rest are bookkeeping.
#[derive(Debug, Clone)]
pub struct NewLink {
    pub provider: String,
    pub entity: String,
    pub local_id: i64,
    pub remote_id: String,
    pub remote_key: String,
    pub remote_updated_at: Option<DateTime<Utc>>,
    pub base_hash: Option<String>,
}

/// Create the table and its indexes if they are not already there. Safe to call
/// on every command; see the module docs for why it is called that often.
pub fn ensure_table(conn: &Connection) -> Result<()> {
    for stmt in DDL {
        conn.execute_batch(stmt)?;
    }
    Ok(())
}

/// The link for a local issue, if it has one.
pub fn by_local(
    conn: &Connection,
    provider: &str,
    entity: &str,
    local_id: i64,
) -> Result<Option<RemoteLink>> {
    let sql = format!(
        "SELECT {COLS} FROM remote_links \
         WHERE provider = ?1 AND entity = ?2 AND local_id = ?3"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params![provider, entity, local_id])?;
    match rows.next()? {
        Some(row) => Ok(Some(read(row)?)),
        None => Ok(None),
    }
}

/// The link for a remote issue, if it has one. Used on import to notice that a
/// remote issue is already on the board under some other local key.
pub fn by_remote(
    conn: &Connection,
    provider: &str,
    entity: &str,
    remote_id: &str,
) -> Result<Option<RemoteLink>> {
    let sql = format!(
        "SELECT {COLS} FROM remote_links \
         WHERE provider = ?1 AND entity = ?2 AND remote_id = ?3"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params![provider, entity, remote_id])?;
    match rows.next()? {
        Some(row) => Ok(Some(read(row)?)),
        None => Ok(None),
    }
}

/// Insert the pairing or refresh the existing one, stamping `last_synced_at`.
///
/// Conflicts on `local_id` and on `remote_id` are both possible and both mean
/// the same thing — this pairing already exists — so the upsert targets the
/// local index and the remote index is left to reject a genuine attempt to
/// point two local issues at one remote issue.
pub fn upsert(conn: &Connection, new: NewLink) -> Result<RemoteLink> {
    let now = time::format_usec(time::now_usec());
    let remote_updated = new.remote_updated_at.map(time::format_usec);

    conn.execute(
        "INSERT INTO remote_links \
           (provider, entity, local_id, remote_id, remote_key, \
            remote_updated_at, base_hash, last_synced_at, inserted_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?8) \
         ON CONFLICT (provider, entity, local_id) DO UPDATE SET \
           remote_id = excluded.remote_id, \
           remote_key = excluded.remote_key, \
           remote_updated_at = excluded.remote_updated_at, \
           base_hash = excluded.base_hash, \
           last_synced_at = excluded.last_synced_at, \
           updated_at = excluded.updated_at",
        params![
            new.provider,
            new.entity,
            new.local_id,
            new.remote_id,
            new.remote_key,
            remote_updated,
            new.base_hash,
            now,
        ],
    )?;

    by_local(conn, &new.provider, &new.entity, new.local_id)?.ok_or(Error::NotFound)
}

/// Every link for a provider, oldest first.
pub fn list(conn: &Connection, provider: &str) -> Result<Vec<RemoteLink>> {
    let sql = format!("SELECT {COLS} FROM remote_links WHERE provider = ?1 ORDER BY id");
    let mut stmt = conn.prepare(&sql)?;
    let mut out = Vec::new();
    let mut rows = stmt.query(params![provider])?;
    while let Some(row) = rows.next()? {
        out.push(read(row)?);
    }
    Ok(out)
}

fn read(row: &Row) -> Result<RemoteLink> {
    let remote_updated_at: Option<String> = row.get(6)?;
    let last_synced_at: String = row.get(8)?;
    Ok(RemoteLink {
        id: row.get(0)?,
        provider: row.get(1)?,
        entity: row.get(2)?,
        local_id: row.get(3)?,
        remote_id: row.get(4)?,
        remote_key: row.get(5)?,
        remote_updated_at: remote_updated_at.as_deref().and_then(time::parse_ts),
        base_hash: row.get(7)?,
        last_synced_at: time::parse_ts(&last_synced_at).unwrap_or_else(time::now_usec),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        ensure_table(&c).unwrap();
        c
    }

    fn new_link(local_id: i64, remote_id: &str, remote_key: &str) -> NewLink {
        NewLink {
            provider: crate::PROVIDER_LINEAR.into(),
            entity: crate::ENTITY_ISSUE.into(),
            local_id,
            remote_id: remote_id.into(),
            remote_key: remote_key.into(),
            remote_updated_at: Some(time::now_usec()),
            base_hash: Some("abc123".into()),
        }
    }

    #[test]
    fn ensure_table_is_idempotent() {
        let c = conn();
        // The second call is the one that matters: a sync command runs it on
        // every invocation, forever.
        ensure_table(&c).unwrap();
        ensure_table(&c).unwrap();
    }

    #[test]
    fn upsert_then_read_back_by_either_side() {
        let c = conn();
        let saved = upsert(&c, new_link(7, "uuid-1", "ENG-412")).unwrap();
        assert_eq!(saved.local_id, 7);
        assert_eq!(saved.remote_key, "ENG-412");

        let by_l = by_local(&c, crate::PROVIDER_LINEAR, crate::ENTITY_ISSUE, 7)
            .unwrap()
            .unwrap();
        let by_r = by_remote(&c, crate::PROVIDER_LINEAR, crate::ENTITY_ISSUE, "uuid-1")
            .unwrap()
            .unwrap();
        assert_eq!(by_l.id, by_r.id);
        assert_eq!(by_l.base_hash.as_deref(), Some("abc123"));
    }

    #[test]
    fn upsert_refreshes_rather_than_duplicating() {
        let c = conn();
        upsert(&c, new_link(7, "uuid-1", "ENG-412")).unwrap();
        // Same local issue, remote key has changed (issue moved team).
        let again = upsert(&c, new_link(7, "uuid-1", "PLAT-9")).unwrap();
        assert_eq!(again.remote_key, "PLAT-9");
        assert_eq!(list(&c, crate::PROVIDER_LINEAR).unwrap().len(), 1);
    }

    #[test]
    fn two_local_issues_cannot_claim_one_remote_issue() {
        let c = conn();
        upsert(&c, new_link(7, "uuid-1", "ENG-412")).unwrap();
        // The remote unique index is the guard: pointing a second local issue
        // at the same Linear issue would make push ambiguous.
        assert!(upsert(&c, new_link(8, "uuid-1", "ENG-412")).is_err());
    }

    #[test]
    fn missing_link_reads_as_none_not_error() {
        let c = conn();
        assert!(
            by_local(&c, crate::PROVIDER_LINEAR, crate::ENTITY_ISSUE, 99)
                .unwrap()
                .is_none()
        );
    }
}
