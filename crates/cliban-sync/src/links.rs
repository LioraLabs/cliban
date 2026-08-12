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
                    remote_updated_at, base_hash, last_synced_at, origin, \
                    progress_comment_id";

/// Who created a pairing. Recorded once, when the link row is first inserted,
/// and never rewritten by later syncs — it answers "whose spec was this
/// originally?", which is what decides `## Spec` ownership on re-import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Origin {
    /// `import linear` created the pairing: the spec came from the remote, so
    /// the remote owns it and a re-import refreshes it.
    #[default]
    Imported,
    /// `push --create` created the pairing: the spec was written on the board,
    /// so cliban owns it and a re-import leaves it alone.
    Pushed,
}

impl Origin {
    pub fn as_str(self) -> &'static str {
        match self {
            Origin::Imported => "imported",
            Origin::Pushed => "pushed",
        }
    }

    /// Lenient on purpose: the database is shared with sibling forks, so a
    /// value this build has never heard of is possible. Falling back to
    /// `Imported` preserves the pre-origin ownership behavior.
    pub fn from_db(s: &str) -> Origin {
        match s {
            "pushed" => Origin::Pushed,
            _ => Origin::Imported,
        }
    }
}

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
    /// Who created the pairing. Fixed at insert; see [`Origin`].
    pub origin: Origin,
    /// The Linear id of the living progress comment `push` maintains, once one
    /// exists. Written only through [`set_progress_comment`]; the upsert never
    /// touches it, so every sync of either direction leaves it alone.
    pub progress_comment_id: Option<String>,
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
    /// Only honored when the row is first inserted; an upsert over an existing
    /// pairing keeps the recorded origin.
    pub origin: Origin,
}

/// Create the table and its indexes if they are not already there. Safe to call
/// on every command; see the module docs for why it is called that often.
pub fn ensure_table(conn: &Connection) -> Result<()> {
    for stmt in DDL {
        conn.execute_batch(stmt)?;
    }
    // `origin` arrived after `remote_links` first shipped, so a table created
    // by an older build — or by a sibling fork's vendored copy of the DDL —
    // exists without the column. Same loom-lockstep rules as the table itself:
    // no `SCHEMA_VERSION` bump, just an additive upgrade applied idempotently
    // before every sync command. SQLite has no `ADD COLUMN IF NOT EXISTS`,
    // hence the pragma check. The DEFAULT backfills existing rows as
    // 'imported', which is what every pre-origin row actually was.
    if !has_column(conn, "remote_links", "origin")? {
        conn.execute_batch(
            "ALTER TABLE remote_links ADD COLUMN origin TEXT NOT NULL DEFAULT 'imported'",
        )?;
    }
    // Same story for the living progress comment's id:
    // is additive too. NULL means "no comment created yet", which is exactly
    // right for every pre-column row.
    if !has_column(conn, "remote_links", "progress_comment_id")? {
        conn.execute_batch("ALTER TABLE remote_links ADD COLUMN progress_comment_id TEXT")?;
    }
    Ok(())
}

/// Whether `table` already has `column`, per `pragma_table_info`.
fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare("SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2")?;
    Ok(stmt.exists(params![table, column])?)
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

    // `origin` is deliberately absent from the DO UPDATE list: it records who
    // *created* the pairing, and every later sync of either direction upserts
    // this same row — updating it would flip spec ownership on the next sync.
    conn.execute(
        "INSERT INTO remote_links \
           (provider, entity, local_id, remote_id, remote_key, \
            remote_updated_at, base_hash, last_synced_at, inserted_at, updated_at, origin) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?8, ?9) \
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
            new.origin.as_str(),
        ],
    )?;

    by_local(conn, &new.provider, &new.entity, new.local_id)?.ok_or(Error::NotFound)
}

/// Record (or clear) the id of the living progress comment on a pairing.
///
/// Separate from [`upsert`] on purpose: the comment id has a different
/// lifecycle from the sync bookkeeping — it is written when `push` creates or
/// recreates the comment, and must survive every other sync untouched.
pub fn set_progress_comment(
    conn: &Connection,
    provider: &str,
    entity: &str,
    local_id: i64,
    comment_id: Option<&str>,
) -> Result<()> {
    let n = conn.execute(
        "UPDATE remote_links SET progress_comment_id = ?4, updated_at = ?5 \
         WHERE provider = ?1 AND entity = ?2 AND local_id = ?3",
        params![
            provider,
            entity,
            local_id,
            comment_id,
            time::format_usec(time::now_usec()),
        ],
    )?;
    if n == 0 {
        return Err(Error::NotFound);
    }
    Ok(())
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
    let origin: String = row.get(9)?;
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
        origin: Origin::from_db(&origin),
        progress_comment_id: row.get(10)?,
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
            origin: Origin::Imported,
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

    #[test]
    fn ensure_table_adds_origin_to_a_pre_origin_table_and_backfills_imported() {
        let c = Connection::open_in_memory().unwrap();
        // The table exactly as it shipped before `origin` existed — what an
        // older build, or a sibling fork's vendored DDL, leaves behind.
        c.execute_batch(
            r#"CREATE TABLE "remote_links" (
                "id" INTEGER PRIMARY KEY AUTOINCREMENT,
                "provider" TEXT NOT NULL,
                "entity" TEXT NOT NULL,
                "local_id" INTEGER NOT NULL,
                "remote_id" TEXT NOT NULL,
                "remote_key" TEXT NOT NULL,
                "remote_updated_at" TEXT,
                "base_hash" TEXT,
                "last_synced_at" TEXT NOT NULL,
                "inserted_at" TEXT NOT NULL,
                "updated_at" TEXT NOT NULL
            )"#,
        )
        .unwrap();
        c.execute(
            "INSERT INTO remote_links \
               (provider, entity, local_id, remote_id, remote_key, last_synced_at, \
                inserted_at, updated_at) \
             VALUES ('linear', 'issue', 7, 'uuid-1', 'ENG-412', '2026-01-01T00:00:00Z', \
                     '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        ensure_table(&c).unwrap();
        // And again: the upgrade has to be as repeatable as the CREATE.
        ensure_table(&c).unwrap();

        let link = by_local(&c, crate::PROVIDER_LINEAR, crate::ENTITY_ISSUE, 7)
            .unwrap()
            .unwrap();
        assert_eq!(
            link.origin,
            Origin::Imported,
            "pre-origin rows were only ever created by import"
        );
    }

    #[test]
    fn upsert_records_origin_and_a_later_sync_never_flips_it() {
        let c = conn();
        let mut first = new_link(7, "uuid-1", "ENG-412");
        first.origin = Origin::Pushed;
        let saved = upsert(&c, first).unwrap();
        assert_eq!(saved.origin, Origin::Pushed);

        // Every later sync upserts the same pairing; origin records who
        // *created* it, not who synced last, so it must survive unchanged.
        let again = upsert(&c, new_link(7, "uuid-1", "ENG-412")).unwrap();
        assert_eq!(again.origin, Origin::Pushed);
    }

    #[test]
    fn ensure_table_adds_progress_comment_id_to_a_pre_column_table() {
        let c = Connection::open_in_memory().unwrap();
        // The table has an origin present but no comment id yet.
        c.execute_batch(
            r#"CREATE TABLE "remote_links" (
                "id" INTEGER PRIMARY KEY AUTOINCREMENT,
                "provider" TEXT NOT NULL,
                "entity" TEXT NOT NULL,
                "local_id" INTEGER NOT NULL,
                "remote_id" TEXT NOT NULL,
                "remote_key" TEXT NOT NULL,
                "remote_updated_at" TEXT,
                "base_hash" TEXT,
                "last_synced_at" TEXT NOT NULL,
                "inserted_at" TEXT NOT NULL,
                "updated_at" TEXT NOT NULL,
                "origin" TEXT NOT NULL DEFAULT 'imported'
            )"#,
        )
        .unwrap();
        c.execute(
            "INSERT INTO remote_links \
               (provider, entity, local_id, remote_id, remote_key, last_synced_at, \
                inserted_at, updated_at) \
             VALUES ('linear', 'issue', 7, 'uuid-1', 'ENG-412', '2026-01-01T00:00:00Z', \
                     '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        ensure_table(&c).unwrap();
        // Repeatable, like every other ensure_table upgrade.
        ensure_table(&c).unwrap();

        let link = by_local(&c, crate::PROVIDER_LINEAR, crate::ENTITY_ISSUE, 7)
            .unwrap()
            .unwrap();
        assert_eq!(
            link.progress_comment_id, None,
            "a pre-column row has no living comment yet"
        );
    }

    #[test]
    fn set_progress_comment_persists_and_reads_back() {
        let c = conn();
        upsert(&c, new_link(7, "uuid-1", "ENG-412")).unwrap();
        set_progress_comment(
            &c,
            crate::PROVIDER_LINEAR,
            crate::ENTITY_ISSUE,
            7,
            Some("comment-uuid-1"),
        )
        .unwrap();
        let link = by_local(&c, crate::PROVIDER_LINEAR, crate::ENTITY_ISSUE, 7)
            .unwrap()
            .unwrap();
        assert_eq!(link.progress_comment_id.as_deref(), Some("comment-uuid-1"));

        // Clearing is how a caller forgets a comment it could not resolve.
        set_progress_comment(&c, crate::PROVIDER_LINEAR, crate::ENTITY_ISSUE, 7, None).unwrap();
        let link = by_local(&c, crate::PROVIDER_LINEAR, crate::ENTITY_ISSUE, 7)
            .unwrap()
            .unwrap();
        assert_eq!(link.progress_comment_id, None);
    }

    #[test]
    fn upsert_never_clobbers_a_stored_comment_id() {
        let c = conn();
        upsert(&c, new_link(7, "uuid-1", "ENG-412")).unwrap();
        set_progress_comment(
            &c,
            crate::PROVIDER_LINEAR,
            crate::ENTITY_ISSUE,
            7,
            Some("comment-uuid-1"),
        )
        .unwrap();

        // Every later sync (import or push) upserts the same pairing; the
        // living comment must survive all of them.
        let again = upsert(&c, new_link(7, "uuid-1", "ENG-412")).unwrap();
        assert_eq!(again.progress_comment_id.as_deref(), Some("comment-uuid-1"));
    }

    #[test]
    fn an_unrecognized_origin_reads_as_imported() {
        // A sibling fork could write a value this build has never heard of;
        // falling back to `imported` preserves today's ownership behavior.
        assert_eq!(Origin::from_db("imported"), Origin::Imported);
        assert_eq!(Origin::from_db("pushed"), Origin::Pushed);
        assert_eq!(Origin::from_db("mystery"), Origin::Imported);
    }
}
