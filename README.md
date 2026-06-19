# cliban

Self-hosted, AI-agent-first kanban board for the terminal.

- A small Rust workspace, one binary, no daemon.
- SQLite (WAL mode) at `$XDG_DATA_HOME/cliban/cliban.db` by default.
- Three front doors: a ratatui TUI (lifted from loom, no daemon/agent machinery),
  a flat CLI, and a Claude Code skill bundle.

## Workspace

- `cliban-core` — storage + domain layer (rusqlite; owns the schema and migrations).
- `cliban-tui` — the kanban board, loom's ratatui frontend rewired to call `cliban-core`
  in-process. Priority-colored bordered cards over cliban's five columns
  (`backlog / in-progress / blocked / in-review / done`).
- `cliban` — the CLI binary; `cliban <subcommand>` for scripting, `cliban` (no args) or
  `cliban tui` to launch the board.

## Quickstart

```bash
cargo build --release
install -m755 target/release/cliban ~/.local/bin/cliban   # or anywhere on PATH
cliban project add CLI --name "Cliban"
cliban issue add --project CLI --title "First issue" --priority high
cliban             # opens the TUI
```

### Board keys

`hjkl` move the cursor · `H/L` move the focused issue across columns · `J/K` reorder it
within a column · `Enter` detail · `e` edit ($EDITOR) · `E` edit project/milestone ·
`n` new issue · `N` new milestone · `t` cycle milestone tag · `a` archive ·
`m` milestone overlay (`Enter` filters to the highlighted milestone) ·
`M` cycle milestone filter · `/` fuzzy find · `r` refresh · `?` help · `q` quit.

## Migrating from the Go cliban

The legacy Go build stored data in the same SQLite file under an older schema. Convert it
once:

```bash
cliban migrate-legacy --from /path/to/old/cliban.db --to /path/to/new/cliban.db
```

It opens the source read-only and writes a fresh `cliban-core` database, preserving
projects, milestones, issues, labels, relations, and done-timestamps.

## Editor integration

By default `cliban issue add` and `cliban issue edit` never open an editor — they fail fast
if no content flags are supplied, which is the right behavior for agents. Pass `--editor`
to opt in to the frontmatter + markdown buffer in `$EDITOR` (`$VISUAL` first, falls back to
`vi`). Inside the TUI, select a card and press `e`.

## Documentation

- Design spec: `docs/specs/2026-05-19-cliban-design.md`
- Skill bundle for Claude Code: `skill/cliban-skill/SKILL.md`

## Description contract

Some cliban commands (`issue tick`, `issue promote`, `issue log`, `issue show --section`) parse the markdown structure of an issue's `description` field. They expect a small, well-defined contract:

### Top-level sections

The following H2 anchors are reserved. The exact heading text matters; cliban looks up sections by exact match.

- `## Spec` — the design/brainstorm output for this issue
- `## Plan` — the implementation plan
- `## Activity Log` — chronological events
- `## Notes` — long-lived notes (mostly for project-level descriptions)

Anything else in the description is preserved untouched.

### Plan tasks and steps

Within `## Plan`, tasks are numbered H3 headings:

```markdown
## Plan

### Task 1: short title

- [ ] **Step 1: ...**
- [ ] **Step 2: ...**

### Task 2: another short title

- [ ] **Step 1: ...**
```

- Tasks are numbered (`### Task <N>:`). Numbers must be unique within the section.
- Steps are GFM checkbox lines at column zero: `- [ ] ...` or `- [x] ...`. Indented child bullets are not parsed as steps.

### Promotion suffix

A step that has been split into its own issue is suffixed with ` → KEY`:

```markdown
- [ ] **Step 3: CSRF middleware** → CLI-18
```

This is produced by `cliban issue promote` and consumed by readers (humans, and any tooling that walks plans).

### Failure mode

If the description structure is violated (missing `## Plan` anchor, renamed `### Task N`, etc.), the workflow commands exit with code 2 and a clear error pointing at the structural problem. No best-effort recovery — fix the description and retry.

## Fuzzy-find tickets

Three coordinated surfaces share one matcher:

- `cliban issue ls --search QUERY` — pipeable. Adds a `score` field in `--json` output; respects every existing `ls` filter (`--project`, `--label`, `--milestone`, `--status`, `--priority`, `--archived`, `--no-subs`, `--parent`). `--limit N` caps results (default 50 when `--search` is set).
- `cliban fff [QUERY]` — prints the selected key to stdout so you can compose: `cliban issue show $(cliban fff)`. Same filter flags as `ls`. Batch NDJSON mode when stdin is not a TTY (great for `cliban fff foo | jq`).
- `/` inside `cliban tui` — fuzzy filter overlay; selecting a card snaps the board cursor onto it.

The matcher weights matches across title (×3.0), key (×2.5), labels (×2.0), and description (×1.0). Default scope is all non-archived issues across all projects; narrow with `--project`, `--label`, etc.

## Test

```bash
cargo test --workspace
```
