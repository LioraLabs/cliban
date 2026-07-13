# cliban

Self-hosted, AI-agent-first kanban board for the terminal.

- A small Rust workspace, one binary, no daemon required.
- SQLite (WAL mode) at `$XDG_DATA_HOME/cliban/cliban.db` by default.
- Three front doors: a ratatui TUI (lifted from loom, no daemon/agent machinery),
  a flat CLI, and a Claude Code skill bundle.
- Optional shared hosting: `cliband`, an SSH daemon that serves the same board
  to your whole team — `ssh boards.example.com` and you're in. See
  [Hosting shared boards](#hosting-shared-boards-over-ssh-cliband).

## Workspace

- `cliban-core` — storage + domain layer (rusqlite; owns the schema and migrations).
- `cliban-tui` — the kanban board, loom's ratatui frontend rewired to call `cliban-core`
  in-process. Priority-colored bordered cards over cliban's five columns
  (`backlog / in-progress / blocked / in-review / done`).
- `cliban` — the CLI binary; `cliban <subcommand>` for scripting, `cliban` (no args) or
  `cliban tui` to launch the board.
- `cliban-tenancy` — multi-tenant storage for the daemon: a `registry.db`
  (users, pubkeys, memberships, invites) routing to one cliban-core database
  per tenant under `tenants/<id>.db`.
- `cliban-server` — the `cliband` binary: a russh-based SSH daemon serving the
  TUI to authenticated clients, with live cross-session updates.

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

## Hosting shared boards over SSH (cliband)

`cliband` turns cliban into a hosted, multi-tenant kanban service with SSH as
the only transport: no browser, no TLS certificates, no reverse proxy. Auth is
SSH public keys; every tenant gets its own SQLite database, so isolation is
physical. Boards are live — a card moved in one session appears in every other
session on that tenant within a tick.

### Five-minute VPS setup

```bash
# on the server
cargo build --release -p cliban-server
sudo install -m755 target/release/cliband /usr/local/bin/cliband
sudo useradd --system --home /var/lib/cliband cliband
sudo mkdir -p /etc/cliband
sudo cp deploy/config.example.toml /etc/cliband/config.toml   # set signup_token!
sudo cp deploy/cliband.service /etc/systemd/system/
sudo systemctl enable --now cliband
```

First boot generates an ed25519 host key under the data dir. Point a DNS name
at the box, then from anywhere:

```bash
ssh -p 2222 boards.example.com signup myteam <signup-token>   # create a tenant
ssh -p 2222 boards.example.com                                # open the board
```

Teammates join with their own keys:

```bash
ssh -p 2222 boards.example.com invite          # you (owner): prints a one-time code
ssh -p 2222 boards.example.com accept <code>   # them: joins as member
```

Other control commands: `whoami`, `members`. A key with several tenants gets a
picker on connect. Running the daemon ad hoc (no systemd) also works:
`cliband --config config.toml`, or plain `cliband` for pure defaults.

### Configuration

All keys optional; defaults in parentheses. See `deploy/config.example.toml`.

| Key | Meaning |
|---|---|
| `listen_addr` | bind address for the SSH listener (`0.0.0.0:2222`) |
| `data_dir` | host key + registry.db + tenants/*.db (`$XDG_DATA_HOME/cliband`) |
| `signup_policy` | `open` \| `token` \| `closed` (`token`) |
| `signup_token` | shared token for `signup_policy = "token"` (unset ⇒ signup denied) |
| `max_tenants_per_key` | tenants one public key may create, 0 = unlimited (`5`) |
| `max_tenants` | global tenant cap, 0 = unlimited (`0`) |

Logs go to stderr, one fact per line — `journalctl -u cliband` shows them
stamped and indexed. Backup/export/delete of a tenant is a file operation on
its `tenants/<id>.db`.

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

## Persistent agent memory

Store durable agent context in the project's existing Markdown description,
under `## Notes`. Give each independently useful lesson its own `###` heading
so cliban can retrieve it without loading the whole section.

```bash
cliban project add CLI --name "Cliban" --description-file project.md
cliban project show CLI --section notes
cliban project search CLI "sqlte canonical" --section notes --json
cliban project edit CLI --description-file updated-project.md
# stdin works too:
cliban project edit CLI --description-file - < updated-project.md
```

`project search` fuzzy-matches every whitespace-separated query term against
each `###` heading and body. It returns only matching subsections as NDJSON,
ranked by score and capped by `--limit` (default 20). This makes retrieval
progressive: search first, then load the full `## Notes` section only when
needed. `--section all` searches every `###` subsection in the description.

## Test

```bash
cargo test --workspace
```
