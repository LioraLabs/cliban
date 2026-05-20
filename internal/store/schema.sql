CREATE TABLE IF NOT EXISTS project (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    key         TEXT    NOT NULL UNIQUE,
    name        TEXT    NOT NULL,
    description TEXT    NOT NULL DEFAULT '',
    archived    INTEGER NOT NULL DEFAULT 0,
    issue_seq   INTEGER NOT NULL DEFAULT 0,
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
    created_at   TEXT    NOT NULL,
    updated_at   TEXT    NOT NULL,
    completed_at TEXT,
    UNIQUE(project_id, seq)
);

CREATE INDEX IF NOT EXISTS idx_issue_project_status ON issue(project_id, status);
CREATE INDEX IF NOT EXISTS idx_issue_parent         ON issue(parent_id);
CREATE INDEX IF NOT EXISTS idx_issue_milestone      ON issue(milestone_id);

CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
