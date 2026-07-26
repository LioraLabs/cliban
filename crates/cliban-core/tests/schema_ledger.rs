//! How `migrations::run` reacts to a `schema_migrations` ledger it did not
//! write. cliban shares its default database path with sibling tools that
//! vendor `cliban-core` (loom, for one), so the ledger routinely carries
//! versions this build has never heard of.

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
    // What loom leaves behind: the legacy baseline plus its own newer version.
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
