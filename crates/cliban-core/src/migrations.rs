//! Migration runner. The project Markdown description remains the durable
//! project-memory store, so no separate notes schema is needed.

use rusqlite::Connection;

pub const LEGACY_SCHEMA_VERSION: i64 = 20260619000001;
pub const SUPERSEDED_NOTES_SCHEMA_VERSION: i64 = 20260713000001;
pub const SCHEMA_VERSION: i64 = 20260713000002;

/// `remote_links` — the local↔remote issue mapping the sync bridges keep.
///
/// # Why adding this table did NOT bump `SCHEMA_VERSION`
///
/// Because the ledger is not cliban's alone to write. Sibling tools vendor a
/// fork of this crate and stamp their own versions into the same
/// `schema_migrations` table in the same default database — and loom's vendored
/// runner, unlike this one, has no escape hatch for a version it does not
/// recognize: it returns `rusqlite::Error::InvalidQuery`, whose Display string
/// is the famously unhelpful "Query is not read-only". Stamping a new cliban
/// version into a shared ledger therefore breaks loom at open time, for every
/// command, until its vendored copy is updated in lockstep.
///
/// A version bump would buy nothing here anyway. The statements below are
/// additive and written entirely with `IF NOT EXISTS`, applied from two
/// directions: the fresh-database path in [`run`], and directly by
/// `cliban-sync::links::ensure_table` before every sync command. The second
/// path is what actually guarantees the table on an existing database — including
/// one whose ledger a sibling owns, where [`run`] deliberately declines to
/// migrate at all. Nothing is left for a ledger entry to do.
///
/// The DDL lives here rather than in `cliban-sync` so there is one copy, and so
/// core does not need to depend on its own dependent.
///
/// If a future change genuinely needs a version bump, check
/// `~/dev/loom/vendor/cliban-core/src/migrations.rs` first: cliban's versions
/// are date-stamped, so simply cutting a release can move cliban's number past
/// the sibling's and flip the sibling's entry from "newer, fine" to "unknown,
/// refuse" in the runner below.
pub const REMOTE_LINKS_DDL: &[&str] = &[
    // `origin` records who created the pairing ('imported' | 'pushed') and
    // decides which side owns the `## Spec` prose. It appears here so fresh
    // databases get it; a table that predates the column (an older build, or a
    // sibling fork's vendored copy of this DDL) is upgraded by the pragma-check
    // ALTER in `cliban-sync::links::ensure_table` — additive, idempotent, and
    // deliberately NOT a `SCHEMA_VERSION` bump, per the essay above.
    r#"CREATE TABLE IF NOT EXISTS "remote_links" (
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
    r#"CREATE UNIQUE INDEX IF NOT EXISTS "remote_links_local_index"
        ON "remote_links" ("provider", "entity", "local_id")"#,
    r#"CREATE UNIQUE INDEX IF NOT EXISTS "remote_links_remote_index"
        ON "remote_links" ("provider", "entity", "remote_id")"#,
];

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

/// `issue_claims` — which actor is holding an issue right now.
///
/// Same deal as [`REMOTE_LINKS_DDL`]: additive, `IF NOT EXISTS`, and NOT a
/// `SCHEMA_VERSION` bump, because the ledger is shared with sibling forks (see
/// the essay above). The fresh-database path in [`run`] applies it, and
/// `contexts::claims::ensure` applies it lazily before every claims read or
/// write — which is what guarantees the table on a database whose ledger a
/// newer sibling owns, where [`run`] declines to migrate at all.
pub const CLAIMS_DDL: &[&str] = &[
    r#"CREATE TABLE IF NOT EXISTS "issue_claims" (
        "issue_id" INTEGER PRIMARY KEY REFERENCES "issues"("id") ON DELETE CASCADE,
        "claimed_by" TEXT NOT NULL,
        "claimed_at" TEXT NOT NULL
    )"#,
];

/// Create the schema on a fresh DB or migrate a recognized prior version.
///
/// Returns `true` when the schema was created or migrated, `false` when the
/// database was already usable as-is.
pub fn run(conn: &Connection) -> crate::error::Result<bool> {
    if !has_ledger(conn)? {
        let tx = conn.unchecked_transaction()?;
        for stmt in SCHEMA_DDL.iter().chain(REMOTE_LINKS_DDL).chain(CLAIMS_DDL) {
            tx.execute_batch(stmt)?;
        }
        stamp(&tx)?;
        tx.commit()?;
        return Ok(true);
    }

    let versions = schema_versions(conn)?;

    // A ledger entry newer than ours means another, newer build owns this
    // database — cliban's sibling tools (and forks that vendor cliban-core)
    // share the default DB path, and their migrations only ever ADD tables.
    // Everything this build reads is still present, so use the database as it
    // stands and do not stamp our own (older) version onto it. Refusing here
    // is what produced the old "Query is not read-only" dead end.
    if versions.iter().any(|version| *version > SCHEMA_VERSION) {
        return Ok(false);
    }

    let unknown: Vec<i64> = versions
        .iter()
        .copied()
        .filter(|version| {
            !matches!(
                *version,
                LEGACY_SCHEMA_VERSION | SUPERSEDED_NOTES_SCHEMA_VERSION | SCHEMA_VERSION
            )
        })
        .collect();
    if !unknown.is_empty() {
        return Err(crate::error::Error::SchemaUnknown {
            found: unknown,
            expected: SCHEMA_VERSION,
        });
    }
    if versions.contains(&SCHEMA_VERSION) {
        return Ok(false);
    }
    if !versions.contains(&LEGACY_SCHEMA_VERSION)
        && !versions.contains(&SUPERSEDED_NOTES_SCHEMA_VERSION)
    {
        return Err(crate::error::Error::SchemaUnknown {
            found: versions,
            expected: SCHEMA_VERSION,
        });
    }

    let tx = conn.unchecked_transaction()?;
    if versions.contains(&SUPERSEDED_NOTES_SCHEMA_VERSION) {
        fold_notes_into_project_markdown(&tx)?;
        tx.execute_batch(
            "DROP TABLE notes; \
             ALTER TABLE projects DROP COLUMN note_seq;",
        )?;
    }
    // Additive and idempotent, so it runs for every recognized prior version
    // without needing to know which one we came from.
    for stmt in REMOTE_LINKS_DDL.iter().chain(CLAIMS_DDL) {
        tx.execute_batch(stmt)?;
    }
    stamp(&tx)?;
    tx.commit()?;
    Ok(true)
}

fn stamp(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO schema_migrations (version, inserted_at) VALUES (?1, ?2)",
        rusqlite::params![SCHEMA_VERSION, ledger_stamp()],
    )?;
    Ok(())
}

fn fold_notes_into_project_markdown(conn: &Connection) -> rusqlite::Result<()> {
    let mut by_project = std::collections::BTreeMap::<i64, Vec<(String, String)>>::new();
    {
        let mut stmt =
            conn.prepare("SELECT project_id, title, body FROM notes ORDER BY project_id, id")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (project_id, title, body) = row?;
            by_project
                .entry(project_id)
                .or_default()
                .push((title, body));
        }
    }

    for (project_id, notes) in by_project {
        let description: String = conn.query_row(
            "SELECT description FROM projects WHERE id = ?1",
            [project_id],
            |row| row.get(0),
        )?;
        let blocks = notes
            .into_iter()
            .map(|(title, body)| {
                let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
                let title = if title.is_empty() {
                    "Migrated note"
                } else {
                    &title
                };
                format!("### {title}\n\n{}", demote_body_headings(body.trim()))
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        let description = append_project_notes(&description, &blocks);
        conn.execute(
            "UPDATE projects SET description = ?1 WHERE id = ?2",
            rusqlite::params![description, project_id],
        )?;
    }
    Ok(())
}

fn demote_body_headings(body: &str) -> String {
    let mut output = String::with_capacity(body.len());
    let mut fence = None;
    for line in body.split_inclusive('\n') {
        if let Some((fence_char, fence_len)) = fence {
            output.push_str(line);
            if is_closing_fence(line, fence_char, fence_len) {
                fence = None;
            }
            continue;
        }
        if let Some(marker) = fence_marker(line) {
            output.push_str(line);
            fence = Some(marker);
            continue;
        }

        let indent_len = line.bytes().take_while(|byte| *byte == b' ').count();
        if indent_len > 3 {
            output.push_str(line);
            continue;
        }
        let (indent, trimmed) = line.split_at(indent_len);
        let hashes = trimmed
            .chars()
            .take_while(|character| *character == '#')
            .count();
        if (1..=3).contains(&hashes) && trimmed.as_bytes().get(hashes) == Some(&b' ') {
            output.push_str(indent);
            output.push_str("####");
            output.push_str(&trimmed[hashes..]);
        } else {
            output.push_str(line);
        }
    }
    output
}

fn fence_marker(line: &str) -> Option<(char, usize)> {
    let trimmed = line.trim_start_matches(' ');
    if line.len() - trimmed.len() > 3 {
        return None;
    }
    let marker = trimmed.chars().next()?;
    if !matches!(marker, '`' | '~') {
        return None;
    }
    let count = trimmed
        .chars()
        .take_while(|character| *character == marker)
        .count();
    (count >= 3).then_some((marker, count))
}

fn is_closing_fence(line: &str, fence_char: char, fence_len: usize) -> bool {
    let Some((marker, count)) = fence_marker(line) else {
        return false;
    };
    if marker != fence_char || count < fence_len {
        return false;
    }
    let trimmed = line.trim_start_matches(' ');
    trimmed[count..]
        .trim_matches([' ', '\t', '\r', '\n'])
        .is_empty()
}

fn append_project_notes(description: &str, blocks: &str) -> String {
    let mut offset = 0;
    let mut notes_start = None;
    let mut notes_end = None;
    let mut fence = None;
    for line in description.split_inclusive('\n') {
        if let Some((fence_char, fence_len)) = fence {
            if is_closing_fence(line, fence_char, fence_len) {
                fence = None;
            }
            offset += line.len();
            continue;
        }
        if let Some(marker) = fence_marker(line) {
            fence = Some(marker);
            offset += line.len();
            continue;
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if notes_start.is_none() {
            if trimmed == "## Notes" {
                notes_start = Some(offset + line.len());
            }
        } else if trimmed.starts_with("## ") {
            notes_end = Some(offset);
            break;
        }
        offset += line.len();
    }

    if notes_start.is_some() {
        let insert_at = notes_end.unwrap_or(description.len());
        let before = &description[..insert_at];
        let separator = if before.ends_with("\n\n") {
            ""
        } else if before.ends_with('\n') {
            "\n"
        } else {
            "\n\n"
        };
        format!(
            "{before}{separator}{blocks}\n\n{}",
            &description[insert_at..]
        )
    } else {
        let separator = if description.is_empty() || description.ends_with("\n\n") {
            ""
        } else if description.ends_with('\n') {
            "\n"
        } else {
            "\n\n"
        };
        format!("{description}{separator}## Notes\n\n{blocks}\n")
    }
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

fn schema_versions(conn: &Connection) -> rusqlite::Result<Vec<i64>> {
    let mut stmt = conn.prepare("SELECT version FROM schema_migrations")?;
    let versions = stmt.query_map([], |row| row.get(0))?.collect();
    versions
}
