# cliban — design spec

**Date:** 2026-05-19
**Status:** Approved, ready for implementation plan
**Owner:** alex (code@lioralabs.dev)

## Summary

`cliban` is a self-hosted, AI-agent-first kanban board that lives entirely in the terminal. It ships as a single Go binary with three front doors:

1. **Bubble Tea TUI** — interactive kanban for the human.
2. **Flat CLI** — scriptable subcommands (`cliban issue add`, `cliban issue mv`, …) for shells and for agents.
3. **Markdown skill bundle** — a `cliban-skill/SKILL.md` that teaches Claude Code (or any skill-aware agent) how to drive the CLI.

There is **no MCP server** in v1. Cross-client portability was traded for simpler scope; agents drive the CLI via the skill instead.

Backing store is **SQLite** at `$XDG_DATA_HOME/cliban/cliban.db` (default `~/.local/share/cliban/cliban.db`) in WAL mode. Single user, single team. No daemon.

## Goals / Non-goals

**Goals**
- Single static Go binary, no external runtime dependencies.
- Agent-driven workflow as a first-class concern: every read command supports `--json`, every mutation has explicit flags, the editor flow is opt-in and guarded against accidental hangs.
- Familiar shape for anyone who has used Linear: project-prefixed issue keys (`CLI-42`), milestones, sub-issues, priorities, a single workflow.
- Editor integration matches `git commit` semantics so it works with nvim, vim, helix, vscode, etc., out of the box.

**Non-goals (v1)**
- Multi-user / multi-team / auth.
- MCP server (skill-based agent integration instead).
- Sync, replication, remote storage.
- Comments, assignees, labels.
- Mouse support in the TUI.
- Cycles / sprints.
- Full-text search.
- Bulk mutation commands.

## Architecture

Single Go binary. No daemon. SQLite is the source of truth and is opened directly by both the CLI and the TUI processes (WAL mode handles read/write concurrency for the single-user case).

```
cliban/
├── cmd/cliban/main.go        # Cobra root; dispatches to TUI or subcommand
├── internal/
│   ├── store/                # The ONLY package that touches SQLite
│   │   ├── schema.sql        # All DDL, embedded via go:embed
│   │   ├── store.go          # Open(), Migrate(), tx helpers, time source
│   │   ├── project.go        # Project CRUD
│   │   ├── milestone.go      # Milestone CRUD
│   │   └── issue.go          # Issue CRUD incl. sub-issues and ID generation
│   ├── domain/               # Pure types and enums; no I/O
│   │   ├── project.go        # Project
│   │   ├── milestone.go      # Milestone, MilestoneStatus
│   │   ├── issue.go          # Issue, Status, Priority
│   │   └── key.go            # IssueKey ("CLI-42") parse/format
│   ├── cli/                  # Cobra subcommands; one file per noun
│   │   ├── root.go
│   │   ├── project.go
│   │   ├── milestone.go
│   │   ├── issue.go
│   │   ├── editor.go         # $EDITOR resolution, temp-file management, TTY guard
│   │   ├── buffer.go         # YAML-frontmatter + markdown serialize/parse
│   │   └── output.go         # Table + JSON renderers
│   └── tui/                  # Bubble Tea
│       ├── app.go            # Root model, view router, key dispatch
│       ├── projects.go       # Project list view
│       ├── board.go          # Kanban board view (5 columns)
│       ├── issue.go          # Issue detail view
│       ├── milestones.go     # Milestone list view
│       ├── editor.go         # tea.ExecProcess wrapper
│       └── styles.go         # Lipgloss styles
├── skill/cliban-skill/
│   └── SKILL.md              # Tells agents how to use the CLI
├── docs/specs/               # Design specs (this file)
└── go.mod
```

**Boundary rules**
- `internal/store` is the only thing that imports `database/sql` or knows SQL exists.
- `internal/domain` has zero dependencies on `store`, `cli`, or `tui` — pure types.
- `internal/cli` and `internal/tui` both consume `internal/store` and `internal/domain`. Neither imports the other.
- Business validation (depth-2 sub-issue rule, status enum, priority enum, milestone-belongs-to-same-project) lives in `internal/store` mutation functions and returns typed errors so both front doors get the same checks.

**Dependencies**
- `github.com/spf13/cobra` — CLI framework.
- `github.com/charmbracelet/bubbletea` + `bubbles` + `lipgloss` — TUI.
- `modernc.org/sqlite` — pure-Go SQLite driver (no CGO; lets us ship one static binary on every platform).
- `gopkg.in/yaml.v3` — YAML frontmatter for the editor buffer.

## Data model

Five tables. All timestamps are UTC ISO-8601 strings (sortable, agent-friendly). Schema lives in `internal/store/schema.sql`, embedded with `go:embed`.

```sql
CREATE TABLE project (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    key         TEXT    NOT NULL UNIQUE,        -- e.g. "CLI"
    name        TEXT    NOT NULL,
    description TEXT    NOT NULL DEFAULT '',
    archived    INTEGER NOT NULL DEFAULT 0,
    issue_seq   INTEGER NOT NULL DEFAULT 0,     -- per-project issue counter
    created_at  TEXT    NOT NULL,
    updated_at  TEXT    NOT NULL
);

CREATE TABLE milestone (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id  INTEGER NOT NULL REFERENCES project(id) ON DELETE CASCADE,
    name        TEXT    NOT NULL,
    description TEXT    NOT NULL DEFAULT '',
    target_date TEXT,                            -- ISO date or NULL
    status      TEXT    NOT NULL DEFAULT 'open', -- open | completed | cancelled
    created_at  TEXT    NOT NULL,
    updated_at  TEXT    NOT NULL,
    UNIQUE(project_id, name)
);

CREATE TABLE issue (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id   INTEGER NOT NULL REFERENCES project(id)  ON DELETE CASCADE,
    milestone_id INTEGER          REFERENCES milestone(id) ON DELETE SET NULL,
    parent_id    INTEGER          REFERENCES issue(id)     ON DELETE CASCADE,
    seq          INTEGER NOT NULL,                  -- per-project; key = project.key || '-' || seq
    title        TEXT    NOT NULL,
    description  TEXT    NOT NULL DEFAULT '',
    status       TEXT    NOT NULL DEFAULT 'backlog',
    priority     TEXT    NOT NULL DEFAULT 'none',   -- none|low|medium|high|urgent
    position     REAL    NOT NULL,                  -- ordering within a column
    created_at   TEXT    NOT NULL,
    updated_at   TEXT    NOT NULL,
    completed_at TEXT,
    UNIQUE(project_id, seq)
);

CREATE INDEX idx_issue_project_status ON issue(project_id, status);
CREATE INDEX idx_issue_parent         ON issue(parent_id);
CREATE INDEX idx_issue_milestone      ON issue(milestone_id);

CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

**Statuses** (hardcoded enum, validated in Go): `backlog`, `in-progress`, `blocked`, `in-review`, `done`. Stored as text. No `status` table — keeps the vocabulary obvious to humans and agents.

**Priorities**: `none`, `low`, `medium`, `high`, `urgent`. Same treatment.

**Issue keys** are computed `{project.key}-{seq}`, never stored. Generated inside a transaction: `UPDATE project SET issue_seq = issue_seq + 1 WHERE id = ? RETURNING issue_seq`, then insert the issue with that `seq`. No gaps from races.

**Sub-issues** = `parent_id`. **Maximum depth is 2**, enforced in Go: creating a sub-issue whose parent already has a non-NULL `parent_id` is rejected. Deleting an issue with sub-issues cascades.

**`position`** is a REAL float for fractional indexing (Linear-style). Inserting between cards at positions 1000 and 2000 gets 1500. Defaults to `max(position) + 1000` when added to a column. Rebalancing strategy when fractional indices grow tight is deferred to v2; for v1 the float precision is enough headroom for any realistic single-user use.

**`meta`** holds `schema_version` and any future global config the app needs without inventing a new table.

## CLI surface

`cliban` with no args opens the TUI. Otherwise it's Cobra subcommands. Every read command supports `--json` (NDJSON for lists, single JSON for `show`). Every mutating command prints the resulting object as JSON when `--json` is set, plain text otherwise.

**Exit codes**: `0` success, `1` not-found, `2` validation error, `3` internal/DB error.

**Top-level**
```
cliban                          # opens TUI
cliban init                     # creates DB + schema at default location
cliban tui                      # explicit TUI launch
cliban version
```

**Projects** — `cliban project <verb>`
```
cliban project add <KEY> --name "Cliban" [--description "..."]
cliban project ls [--archived] [--json]
cliban project show <KEY> [--json]
cliban project edit <KEY> [--name ...] [--description ...]
cliban project archive <KEY>
cliban project unarchive <KEY>
cliban project rm <KEY>          # requires --force if it has issues
```

**Milestones** — `cliban milestone <verb>` (always scoped to a project; addressed by `(project, name)`)
```
cliban milestone add --project CLI --name "v0.1" [--target 2026-06-01] [--description ...]
cliban milestone ls --project CLI [--status open|completed|cancelled] [--json]
cliban milestone show --project CLI --name "v0.1" [--json]
cliban milestone edit --project CLI --name "v0.1" [--target ...] [--status ...] [--rename "v0.2"]
cliban milestone rm --project CLI --name "v0.1"
```

**Issues** — `cliban issue <verb>` (addressed by key: `CLI-42`)
```
cliban issue add --project CLI --title "..." [--description ...]
                 [--parent CLI-12]            # makes it a sub-issue (max depth 2)
                 [--milestone "v0.1"]
                 [--priority none|low|medium|high|urgent]
                 [--status backlog|in-progress|blocked|in-review|done]
cliban issue ls  [--project CLI] [--status ...] [--priority ...]
                 [--milestone "v0.1"] [--parent CLI-12] [--no-subs]
                 [--json]
cliban issue show CLI-42 [--json]              # includes sub-issues inline
cliban issue edit CLI-42 [--title ...] [--description ...] [--priority ...]
                         [--milestone "v0.1"|--clear-milestone]
                         [--parent CLI-12|--clear-parent]
                         [-e|--edit]            # force editor even with other flags
cliban issue mv  CLI-42 <status>                # the fast path agents use most
cliban issue rm  CLI-42                          # cascades to sub-issues
```

**Stdin for descriptions**: `--description -` reads stdin so agents can pipe multiline markdown without shell-escape pain. Same for `cliban issue edit --description -`.

**Not in v1**: bulk `mv`, `search`, comments, labels, assignees.

## Editor integration

Standard `$EDITOR` flow, modeled exactly on `git commit`. Works with nvim, vim, helix, vscode, anything — no editor-specific code paths.

**When the editor opens**
- `cliban issue add --project CLI` with no `--title` and no `--description` → opens editor with a fresh template.
- `cliban issue edit CLI-42` with no edit flags → opens editor with the current issue serialized.
- `-e` / `--edit` flag forces the editor open even when other flags are set.

**Buffer format** (YAML frontmatter + markdown body):

```
# Editing CLI-42 — lines above the first '---' are ignored.
# Statuses:   backlog | in-progress | blocked | in-review | done
# Priorities: none | low | medium | high | urgent
# Set milestone or parent to '' (empty) to clear it.
---
title:     Fix the kanban column ordering
status:    in-progress
priority:  high
milestone: v0.1
parent:    CLI-12
---
Description in markdown.

Multiple paragraphs, code fences, whatever.
```

Header `#` comments are only stripped *above* the first `---`, so `#` markdown headers inside the description work fine. The file is named `cliban-issue-{key}.{tmp}.md` so editors infer `markdown` filetype automatically.

**Cancellation semantics**: empty file or zero changes → abort with exit code 0 and "no changes" message. Same as `git commit`.

**Editor resolution order**: `$VISUAL` → `$EDITOR` → `vi`. A `~/.config/cliban/config.toml` can pin one (`editor = "nvim"`); env vars win.

**Agent-safety guards**
- If stdin is not a TTY and the editor would otherwise be triggered → fail fast with exit code 2 and a hint to pass `--title`/`--description`.
- `--no-editor` flag and `$CLIBAN_NO_EDITOR=1` disable the editor unconditionally.
- The skill bundle teaches agents to always pass content flags.

## TUI

Bubble Tea. Three top-level views plus a milestone overlay. Vim-flavored bindings. Keyboard-only (no mouse in v1).

**View 1 — Project list (entry screen)**
```
┌─ cliban ───────────────────────────────────────────┐
│ ▸ CLI   Cliban             12 open    3 in-flight  │
│   FOO   Foo platform        4 open    0 in-flight  │
│   BAR   Bar service         0 open    0 in-flight  │
├────────────────────────────────────────────────────┤
│ j/k move  enter open  n new  a archive  q quit     │
└────────────────────────────────────────────────────┘
```

**View 2 — Kanban board (main screen)**
Five columns, one per status, horizontally scrollable on narrow terminals. Cards show key + title + priority dot + sub-issue count.

```
┌─ CLI ─ Cliban ──────────────────────────────────────────────────┐
│ BACKLOG (4)  │ IN-PROG (2) │ BLOCKED  │ IN-REV (1) │ DONE (8)  │
│ ▸CLI-12 ●H   │ CLI-3  ●M   │ CLI-9 ●L │ CLI-7 ●H   │ CLI-1 ✓   │
│  Fix order   │ Skill setup │ DB lock  │ Editor flow│ Init cmd  │
│ CLI-13 ●M    │ CLI-5  ●U(2)│          │            │ CLI-2 ✓   │
│ Sub-issues   │ Frontmatter │          │            │ Schema    │
├─────────────────────────────────────────────────────────────────┤
│ hjkl move  enter detail  e edit  n new  Space mv  / filter  ? help │
└─────────────────────────────────────────────────────────────────┘
```

Bindings:
- `h`/`l` move between columns; `j`/`k` within. Selection persists per column.
- `Space` then `b`/`i`/`k`/`r`/`d` moves the selected card to backlog/in-progress/blocked/in-review/done.
- `J`/`K` (capital) moves a card up/down within its column (adjusts `position`).
- `H`/`L` (capital) shoves a card one column left/right.
- `n` opens the editor with a blank template, defaulting the status to the current column.
- `e` opens the selected issue in the editor.
- `enter` → issue detail view.
- `/` filters by substring across title; `Esc` clears.
- `f` cycles a filter chip set (milestone, priority, parent-only, sub-only).
- `r` forces a re-read from SQLite (for picking up external mutations).
- `?` overlay shows all bindings.
- `q` / `Esc` returns to project list.

**View 3 — Issue detail**
Title, metadata block, full description rendered as wrapped markdown, indented list of sub-issues. `e` opens it in the editor. `Esc`/`q` returns to the board with selection preserved.

**Milestone overlay** — toggled with `m` from any view. Lists milestones for the current project with progress bars and target dates. `enter` filters the board to that milestone. `e` opens the milestone in the editor with a simpler frontmatter (name/target/status/description).

**Editor from inside the TUI** uses `tea.ExecProcess`:
1. Serialize the current issue (or blank template) to `cliban-issue-{key}.{tmp}.md`.
2. Bubble Tea suspends, restores the terminal to a sane state, execs `$EDITOR <tempfile>` as a foreground child sharing the TTY.
3. On editor exit, the TUI resumes, reads the file, parses the buffer format, applies changes through `internal/store` (same code path as `cliban issue edit`).
4. Unchanged or empty → silent return. Parse/validation error → bottom-row toast; the temp file is preserved at a path printed in the toast for recovery.

Works identically with nvim, vim, helix, micro, even `code --wait`.

**Live external-change refresh** is **not** automatic. If an agent edits the DB while the TUI is open, you see the change on next view switch or `r`. SQLite WAL guarantees consistency.

## Skill bundle

`skill/cliban-skill/SKILL.md` is shipped in the repo and installed by the user into their Claude Code skills directory. The skill teaches the agent:

- The full CLI surface (lifted from this spec).
- That `--json` is the right call for any read; never parse table output.
- That mutations should always include content flags; never trigger the editor (or use `--no-editor`).
- The status and priority vocabularies.
- The depth-2 sub-issue rule.
- How to resolve `CLI-42` ↔ project key.
- Common recipes: "create issue, add sub-issue, move to in-progress", "list everything for milestone v0.1", "find all blocked issues across projects".

Frontmatter follows the user's existing skill conventions (name, description, when-to-use).

## Error handling

- All `internal/store` mutations return typed errors: `ErrNotFound`, `ErrValidation`, `ErrConflict`, `ErrInternal`.
- CLI handlers translate those into exit codes per the table in §CLI surface.
- TUI handlers translate those into bottom-row toasts; no panics escape the event loop.
- SQLite errors that aren't classifiable bubble up as `ErrInternal` with the original SQL error preserved in `errors.Unwrap`.

## Testing strategy

- **Unit tests** for `internal/store` against a temp SQLite file per test (parallel-safe). Covers: ID generation, sub-issue depth rule, milestone/project FK behavior, position math.
- **Unit tests** for `internal/cli/buffer.go` covering frontmatter round-trip, comment stripping, clear-flag behavior, parse errors.
- **Golden tests** for CLI output (`--json` and plain text) against a fixture DB.
- **TUI snapshot tests** using `teatest` for the three views in their default states.
- **No end-to-end editor-flow tests** in v1 — the editor invocation is a thin shell; we test buffer parsing separately.

## Open questions deferred to v2

- MCP server as a thin wrapper over `internal/store` (cross-client portability).
- Full-text search (FTS5 virtual table).
- Bulk `mv`.
- `cliban issue --print` / `cliban issue apply -` round-trip pipe path.
- Live external-change refresh in the TUI (file watch on the DB).
- Position rebalancing strategy when fractional indices get tight.
- Configurable workflow statuses per project.

## Acceptance criteria for v1

1. `go build` produces a single static binary on Linux/macOS with no CGO.
2. `cliban init` creates a fresh DB and `cliban project add CLI --name Cliban` works.
3. Full create→sub-issue→move→edit→delete lifecycle works via CLI with both human and `--json` output.
4. `cliban issue add` with no flags opens `$EDITOR`; saving creates an issue; aborting creates nothing.
5. TUI opens, shows projects, drills into the board, moves cards with `Space + d`, opens an issue in nvim via `e`, returns to the TUI on editor exit with the change reflected.
6. Sub-issue depth-2 rule rejects a third level with a clear error from both CLI and TUI.
7. `--no-editor` and non-TTY stdin both prevent editor invocation.
8. The skill bundle is present in `skill/cliban-skill/SKILL.md` and is enough for Claude Code to drive the tool without re-reading this spec.
