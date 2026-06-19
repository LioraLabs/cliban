//! Migration runner. cliban-core owns a fresh schema (no live loom.db to
//! recognize), so this is a single forward CREATE pass keyed on a one-row
//! `schema_migrations` ledger. `run` is a no-op once that row is present.

use rusqlite::Connection;

pub const SCHEMA_VERSION: i64 = 20260619000001;

const SCHEMA_DDL: &[&str] = &[
    r#"CREATE TABLE IF NOT EXISTS "schema_migrations" (
        "version" INTEGER PRIMARY KEY,
        "inserted_at" TEXT
    )"#,
    r#"CREATE TABLE "projects" (
        "id" INTEGER PRIMARY KEY AUTOINCREMENT,
        "key" TEXT NOT NULL,
        "name" TEXT NOT NULL,
        "description" TEXT DEFAULT '' NOT NULL,
        "archived" INTEGER DEFAULT 0 NOT NULL,
        "auto_archive_done_after_days" INTEGER,
        "issue_seq" INTEGER DEFAULT 0 NOT NULL,
        "inserted_at" TEXT NOT NULL,
        "updated_at" TEXT NOT NULL
    )"#,
    r#"CREATE UNIQUE INDEX "projects_key_index" ON "projects" ("key")"#,
    r#"CREATE TABLE "milestones" (
        "id" INTEGER PRIMARY KEY AUTOINCREMENT,
        "project_id" INTEGER NOT NULL REFERENCES "projects"("id") ON DELETE CASCADE,
        "name" TEXT NOT NULL,
        "description" TEXT DEFAULT '' NOT NULL,
        "target_date" TEXT,
        "status" TEXT DEFAULT 'open' NOT NULL,
        "archived" INTEGER DEFAULT 0 NOT NULL,
        "inserted_at" TEXT NOT NULL,
        "updated_at" TEXT NOT NULL
    )"#,
    r#"CREATE UNIQUE INDEX "milestones_project_id_name_index" ON "milestones" ("project_id", "name")"#,
    r#"CREATE INDEX "milestones_project_id_index" ON "milestones" ("project_id")"#,
    r#"CREATE TABLE "labels" (
        "id" INTEGER PRIMARY KEY AUTOINCREMENT,
        "project_id" INTEGER NOT NULL REFERENCES "projects"("id") ON DELETE CASCADE,
        "name" TEXT NOT NULL,
        "inserted_at" TEXT NOT NULL,
        "updated_at" TEXT NOT NULL
    )"#,
    r#"CREATE UNIQUE INDEX "labels_project_id_name_index" ON "labels" ("project_id", "name")"#,
    r#"CREATE TABLE "issues" (
        "id" INTEGER PRIMARY KEY AUTOINCREMENT,
        "key" TEXT NOT NULL,
        "project_id" INTEGER NOT NULL REFERENCES "projects"("id") ON DELETE CASCADE,
        "milestone_id" INTEGER REFERENCES "milestones"("id") ON DELETE SET NULL,
        "parent_id" INTEGER REFERENCES "issues"("id") ON DELETE CASCADE,
        "title" TEXT NOT NULL,
        "description" TEXT DEFAULT '' NOT NULL,
        "status" TEXT DEFAULT 'backlog' NOT NULL
          CHECK (status IN ('backlog', 'in-progress', 'blocked', 'in-review', 'done')),
        "priority" TEXT DEFAULT 'none' NOT NULL,
        "position" NUMERIC DEFAULT 0.0 NOT NULL,
        "archived" INTEGER DEFAULT 0 NOT NULL,
        "due_date" TEXT,
        "completed_at" TEXT,
        "inserted_at" TEXT NOT NULL,
        "updated_at" TEXT NOT NULL
    )"#,
    r#"CREATE UNIQUE INDEX "issues_key_index" ON "issues" ("key")"#,
    r#"CREATE INDEX "issues_project_id_index" ON "issues" ("project_id")"#,
    r#"CREATE INDEX "issues_milestone_id_index" ON "issues" ("milestone_id")"#,
    r#"CREATE INDEX "issues_parent_id_index" ON "issues" ("parent_id")"#,
    r#"CREATE INDEX "issues_status_index" ON "issues" ("status")"#,
    r#"CREATE TABLE "issues_labels" (
        "issue_id" INTEGER NOT NULL REFERENCES "issues"("id") ON DELETE CASCADE,
        "label_id" INTEGER NOT NULL REFERENCES "labels"("id") ON DELETE CASCADE,
        PRIMARY KEY ("issue_id","label_id")
    )"#,
    r#"CREATE INDEX "issues_labels_label_id_index" ON "issues_labels" ("label_id")"#,
    r#"CREATE TABLE "activity_log_entries" (
        "id" INTEGER PRIMARY KEY AUTOINCREMENT,
        "issue_id" INTEGER NOT NULL REFERENCES "issues"("id") ON DELETE CASCADE,
        "ts" TEXT NOT NULL,
        "kind" TEXT NOT NULL,
        "message" TEXT DEFAULT '' NOT NULL,
        "extra" TEXT DEFAULT '{}' NOT NULL,
        "inserted_at" TEXT NOT NULL,
        "updated_at" TEXT NOT NULL
    )"#,
    r#"CREATE INDEX "activity_log_entries_issue_id_ts_index" ON "activity_log_entries" ("issue_id", "ts")"#,
    r#"CREATE TABLE "issue_relation" (
        "id" INTEGER PRIMARY KEY AUTOINCREMENT,
        "from_issue_id" INTEGER NOT NULL REFERENCES "issues"("id") ON DELETE CASCADE,
        "to_issue_id" INTEGER NOT NULL REFERENCES "issues"("id") ON DELETE CASCADE,
        "type" TEXT NOT NULL CHECK (type IN ('blocks', 'related_to')),
        "created_at" TEXT NOT NULL,
        UNIQUE ("from_issue_id", "to_issue_id", "type")
    )"#,
    r#"CREATE INDEX "issue_relation_to_index" ON "issue_relation" ("to_issue_id", "type")"#,
];

/// Create the schema on a fresh DB and stamp the version. No-op if the version
/// row is already present. Returns whether DDL was applied.
pub fn run(conn: &Connection) -> rusqlite::Result<bool> {
    if is_up_to_date(conn)? {
        return Ok(false);
    }
    let tx = conn.unchecked_transaction()?;
    for stmt in SCHEMA_DDL {
        tx.execute_batch(stmt)?;
    }
    tx.execute(
        "INSERT OR IGNORE INTO schema_migrations (version, inserted_at) VALUES (?1, ?2)",
        rusqlite::params![SCHEMA_VERSION, ledger_stamp()],
    )?;
    tx.commit()?;
    Ok(true)
}

fn ledger_stamp() -> String {
    crate::time::format_usec(crate::time::now_usec())
        .trim_end_matches('Z')
        .to_string()
}

fn has_ledger(conn: &Connection) -> rusqlite::Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_migrations'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false))
}

fn is_up_to_date(conn: &Connection) -> rusqlite::Result<bool> {
    if !has_ledger(conn)? {
        return Ok(false);
    }
    Ok(conn
        .query_row(
            "SELECT 1 FROM schema_migrations WHERE version = ?1",
            rusqlite::params![SCHEMA_VERSION],
            |_| Ok(true),
        )
        .unwrap_or(false))
}
