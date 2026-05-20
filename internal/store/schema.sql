CREATE TABLE IF NOT EXISTS project (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    key         TEXT    NOT NULL UNIQUE,
    name        TEXT    NOT NULL,
    description TEXT    NOT NULL DEFAULT '',
    archived    INTEGER NOT NULL DEFAULT 0,
    issue_seq   INTEGER NOT NULL DEFAULT 0,
    auto_archive_done_after_days INTEGER,
    created_at  TEXT    NOT NULL,
    updated_at  TEXT    NOT NULL
);

CREATE TABLE IF NOT EXISTS milestone (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id  INTEGER NOT NULL REFERENCES project(id) ON DELETE CASCADE,
    name        TEXT    NOT NULL,
    description TEXT    NOT NULL DEFAULT '',
    target_date TEXT,
    status      TEXT    NOT NULL DEFAULT 'open',
    created_at  TEXT    NOT NULL,
    updated_at  TEXT    NOT NULL,
    UNIQUE(project_id, name)
);

CREATE TABLE IF NOT EXISTS issue (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id   INTEGER NOT NULL REFERENCES project(id)  ON DELETE CASCADE,
    milestone_id INTEGER          REFERENCES milestone(id) ON DELETE SET NULL,
    parent_id    INTEGER          REFERENCES issue(id)     ON DELETE CASCADE,
    seq          INTEGER NOT NULL,
    title        TEXT    NOT NULL,
    description  TEXT    NOT NULL DEFAULT '',
    status       TEXT    NOT NULL DEFAULT 'backlog',
    priority     TEXT    NOT NULL DEFAULT 'none',
    position     REAL    NOT NULL,
    archived     INTEGER NOT NULL DEFAULT 0,
    due_date     TEXT,
    created_at   TEXT    NOT NULL,
    updated_at   TEXT    NOT NULL,
    completed_at TEXT,
    UNIQUE(project_id, seq)
);

CREATE INDEX IF NOT EXISTS idx_issue_project_status ON issue(project_id, status);
CREATE INDEX IF NOT EXISTS idx_issue_parent         ON issue(parent_id);
CREATE INDEX IF NOT EXISTS idx_issue_milestone      ON issue(milestone_id);

CREATE TABLE IF NOT EXISTS label (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id INTEGER NOT NULL REFERENCES project(id) ON DELETE CASCADE,
    name       TEXT    NOT NULL,
    created_at TEXT    NOT NULL,
    UNIQUE(project_id, name)
);

CREATE TABLE IF NOT EXISTS issue_label (
    issue_id INTEGER NOT NULL REFERENCES issue(id) ON DELETE CASCADE,
    label_id INTEGER NOT NULL REFERENCES label(id) ON DELETE CASCADE,
    PRIMARY KEY (issue_id, label_id)
);

CREATE INDEX IF NOT EXISTS idx_issue_label_label ON issue_label(label_id);

CREATE TABLE IF NOT EXISTS issue_relation (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    from_issue_id INTEGER NOT NULL REFERENCES issue(id) ON DELETE CASCADE,
    to_issue_id   INTEGER NOT NULL REFERENCES issue(id) ON DELETE CASCADE,
    type          TEXT    NOT NULL CHECK(type IN ('blocks','related_to')),
    created_at    TEXT    NOT NULL,
    UNIQUE(from_issue_id, to_issue_id, type)
);

CREATE INDEX IF NOT EXISTS idx_issue_relation_to ON issue_relation(to_issue_id, type);

CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
