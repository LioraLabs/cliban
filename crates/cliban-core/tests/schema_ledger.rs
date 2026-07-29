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

fn has_table(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |r| r.get::<_, i64>(0),
    )
    .unwrap()
        > 0
}

/// The version loom's vendored fork of cliban-core stamps.
const LOOM_SCHEMA_VERSION: i64 = 20260713000003;

// A constant invariant, so it is checked when this file compiles rather than
// when the suite runs.
//
// loom vendors a fork of cliban-core and writes the same default database, and
// its runner has NO escape hatch for a version it does not recognize — it
// returns `rusqlite::Error::InvalidQuery` ("Query is not read-only") at open
// time, for every command. A cliban release that stamps a higher version into
// the shared ledger therefore breaks loom until its vendored copy is updated in
// lockstep.
//
// Adding `remote_links` deliberately did not bump the version: the DDL is
// additive and applied idempotently by `cliban-sync::links::ensure_table` on
// every sync command, so no ledger entry is needed. If this stops compiling,
// read the comment on `REMOTE_LINKS_DDL` before raising the bound.
const _: () = assert!(
    SCHEMA_VERSION < LOOM_SCHEMA_VERSION,
    "cliban's schema version has overtaken loom's; loom's vendored runner will \
     refuse the shared database at open time"
);

#[test]
fn a_loom_written_ledger_is_used_as_is_and_not_stamped() {
    let conn = db_with_ledger(&[migrations::LEGACY_SCHEMA_VERSION, LOOM_SCHEMA_VERSION]);
    assert!(
        !migrations::run(&conn).unwrap(),
        "loom's version is ahead; use the database as it stands"
    );
    let stamped: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version = ?1",
            [SCHEMA_VERSION],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stamped, 0, "must not stamp our version into loom's ledger");
}

#[test]
fn a_fresh_database_gets_remote_links() {
    let conn = Connection::open_in_memory().unwrap();
    assert!(migrations::run(&conn).unwrap());
    assert!(has_table(&conn, "remote_links"));
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
