use cliban_core::migrations::{run, SCHEMA_VERSION, SUPERSEDED_NOTES_SCHEMA_VERSION};
use rusqlite::Connection;

#[test]
fn superseded_notes_are_folded_into_project_markdown() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(&format!(
        "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, inserted_at TEXT); \
         INSERT INTO schema_migrations VALUES ({SUPERSEDED_NOTES_SCHEMA_VERSION}, 'stamp'); \
         CREATE TABLE projects (id INTEGER PRIMARY KEY, key TEXT NOT NULL, name TEXT NOT NULL, \
           description TEXT DEFAULT '' NOT NULL, archived INTEGER DEFAULT 0 NOT NULL, \
           auto_archive_done_after_days INTEGER, issue_seq INTEGER DEFAULT 0 NOT NULL, \
           note_seq INTEGER DEFAULT 0 NOT NULL, inserted_at TEXT NOT NULL, updated_at TEXT NOT NULL); \
         CREATE TABLE notes (id INTEGER PRIMARY KEY, project_id INTEGER NOT NULL, key TEXT NOT NULL, \
           title TEXT NOT NULL, body TEXT DEFAULT '' NOT NULL, inserted_at TEXT NOT NULL, updated_at TEXT NOT NULL); \
         INSERT INTO projects VALUES (1, 'MEM', 'Memory', \
           '## Spec\n\nKeep me.\n\n## Notes\n\n### Existing\n\nStill here.\n\n## Tail\n\nAlso keep me.\n', \
           0, NULL, 0, 2, 'stamp', 'stamp'); \
         INSERT INTO notes VALUES (1, 1, 'MEM-N1', 'Database', 'SQLite is canonical.\n\n## Embedded\n\n### Child\n\n    ## indented code', 'stamp', 'stamp'); \
         INSERT INTO notes VALUES (2, 1, 'MEM-N2', 'Multi\nline', 'Flatten the heading.', 'stamp', 'stamp');"
    ))
    .unwrap();

    assert!(run(&conn).unwrap());

    let description: String = conn
        .query_row("SELECT description FROM projects WHERE id = 1", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert!(description.contains("## Spec\n\nKeep me."));
    assert!(description.contains("### Existing\n\nStill here."));
    assert!(description.contains("### Database\n\nSQLite is canonical."));
    assert!(description.contains("#### Embedded"));
    assert!(description.contains("#### Child"));
    assert!(description.contains("    ## indented code"));
    assert!(description.contains("### Multi line\n\nFlatten the heading."));
    assert!(description.find("### Database").unwrap() < description.find("## Tail").unwrap());
    assert!(description.contains("## Tail\n\nAlso keep me."));

    let notes_table: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'notes'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let note_seq: i64 = conn
        .query_row(
            "SELECT count(*) FROM pragma_table_info('projects') WHERE name = 'note_seq'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let current_stamp: i64 = conn
        .query_row(
            "SELECT count(*) FROM schema_migrations WHERE version = ?1",
            [SCHEMA_VERSION],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!((notes_table, note_seq, current_stamp), (0, 0, 1));
    assert!(!run(&conn).unwrap());
}
