//! How `migrations::run` reacts to a `schema_migrations` ledger it did not
//! write — an older binary opening a database a newer one migrated, or a fork
//! that vendors `cliban-core` against the same default database path.

use cliban_core::migrations::{self, SCHEMA_VERSION};
use rusqlite::Connection;

/// A database at the current schema, with `versions` as its ledger.
fn db_with_ledger(versions: &[i64]) -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    migrations::run(&conn).unwrap();
    conn.execute("DELETE FROM schema_migrations", []).unwrap();
    for v in versions {
        conn.execute(
            "INSERT INTO schema_migrations (version, inserted_at) VALUES (?1, '2026-01-01T00:00:00')",
            [v],
        )
        .unwrap();
    }
    conn
}

#[test]
fn a_newer_schema_is_used_as_is_rather_than_refused() {
    // What a newer build leaves behind: the baseline plus its own version.
    let conn = db_with_ledger(&[migrations::LEGACY_SCHEMA_VERSION, SCHEMA_VERSION + 1]);
    assert!(
        !migrations::run(&conn).unwrap(),
        "a newer ledger entry means another build owns the DB; use it, don't migrate"
    );
    // And we must not stamp our own older version onto someone else's DB.
    let stamped: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version = ?1",
            [SCHEMA_VERSION],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stamped, 0);
}

#[test]
fn an_unknown_older_version_is_a_readable_error() {
    let conn = db_with_ledger(&[12345]);
    let err = migrations::run(&conn).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("unrecognized database schema version"),
        "{msg}"
    );
    assert!(msg.contains("12345"), "names the offending version: {msg}");
    assert!(
        !msg.contains("read-only"),
        "must not resurface rusqlite's misleading InvalidQuery text: {msg}"
    );
}

#[test]
fn the_current_version_needs_no_migration() {
    let conn = db_with_ledger(&[SCHEMA_VERSION]);
    assert!(!migrations::run(&conn).unwrap());
}

fn has_table(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |r| r.get::<_, i64>(0),
    )
    .unwrap()
        > 0
}

#[test]
fn a_fresh_database_gets_remote_links() {
    let conn = Connection::open_in_memory().unwrap();
    assert!(migrations::run(&conn).unwrap());
    assert!(has_table(&conn, "remote_links"));
    // Including the columns added after the table first shipped — the fresh
    // path must land where `cliban-sync::links::ensure_table` upgrades to.
    let has_origin: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('remote_links') WHERE name = 'origin'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(has_origin, 1);
}

#[test]
fn a_legacy_database_folds_forward_to_remote_links() {
    let conn = db_with_ledger(&[migrations::LEGACY_SCHEMA_VERSION]);
    conn.execute("DROP TABLE remote_links", []).unwrap();
    assert!(!has_table(&conn, "remote_links"));

    assert!(migrations::run(&conn).unwrap(), "should migrate");
    assert!(has_table(&conn, "remote_links"));
}

#[test]
fn migrating_forward_preserves_existing_rows() {
    let conn = db_with_ledger(&[migrations::LEGACY_SCHEMA_VERSION]);
    conn.execute(
        "INSERT INTO projects (key, name, description, inserted_at, updated_at) \
         VALUES ('CLI', 'Cliban', 'desc', '2026-01-01T00:00:00', '2026-01-01T00:00:00')",
        [],
    )
    .unwrap();
    migrations::run(&conn).unwrap();
    let name: String = conn
        .query_row("SELECT name FROM projects WHERE key = 'CLI'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(name, "Cliban");
}

#[test]
fn remote_links_ddl_is_idempotent_against_a_newer_owner() {
    // The case the direct-apply path in `cliban-sync` exists for: a sibling
    // tool's newer ledger entry makes `run` decline to migrate, so the
    // migration never creates the table. Applying the DDL directly must work
    // on that database, and must be safe to repeat forever.
    let conn = db_with_ledger(&[migrations::LEGACY_SCHEMA_VERSION, SCHEMA_VERSION + 1]);
    conn.execute("DROP TABLE remote_links", []).unwrap();
    assert!(!migrations::run(&conn).unwrap(), "declines to migrate");
    assert!(
        !has_table(&conn, "remote_links"),
        "and so the table is genuinely absent — this is the gap"
    );

    for _ in 0..3 {
        for stmt in migrations::REMOTE_LINKS_DDL {
            conn.execute_batch(stmt).unwrap();
        }
    }
    assert!(has_table(&conn, "remote_links"));
}
