---
name: cliban
description: Drive the local cliban kanban board via its CLI. Use when the user mentions cliban, kanban, ticket, issue, project, milestone, or asks you to capture/move work items, or asks what changed/what happened on the board.
---

# Using cliban

`cliban` is a self-hosted, terminal-first kanban board with a flat CLI. It is
built for agents: every read has a `--json` form, no command opens an editor
unless you ask, and mutations are safe to run unattended.

## Read this first — the six things that trip agents up

1. **`--description` / `--description-file` REPLACES the whole description.**
   It destroys `## Activity Log`, `## Plan` and everything else already there.
   To record progress use `cliban issue log`; to tick a plan step use `cliban
   issue tick`. If you really must rewrite, read the current description first
   (`issue show KEY --json`) and include the sections you want to keep. The
   timeline records the destruction (`"description rewritten, dropped ##
   Plan"`) and logged notes survive it, but the markdown is gone.
2. **Always pass `--json` for reads.** The table format is for humans and will
   change. `ls` emits NDJSON (one compact object per line), `show` emits one
   pretty object.
3. **`--project` takes the KEY, not the name**: `--project CLI`, not
   `--project Cliban`. Keys are uppercase; the CLI upcases what you pass.
4. **`project add` takes the key positionally**: `cliban project add CLI --name
   "Cliban"`. There is no `--key` flag. (`issue add` is the opposite — it needs
   `--project CLI --title "..."`.)
5. **Time arguments accept `3d` / `yesterday` / `2026-07-25` / RFC3339** —
   `--since` and `--updated-since` share one parser, so anything one takes the
   other takes.
6. **The status vocabulary is fixed.** `backlog | in-progress | blocked |
   in-review | done`. Anything else is exit code 2. Move with
   `cliban issue mv KEY done` (the subcommand is `mv`, not `move`).

## The working protocol

cliban records *what* changed on its own. Every mutation lands on the issue's
timeline automatically — moves, archives, field edits (with before → after),
label and relation changes, plan ticks, promotions, and your own `issue log`
notes. You never have to remember to record *what* happened.

It cannot record **why**, and why is the part the next agent (or the human)
actually needs. That is your job.

**Identify yourself once per session**, so a shared board stays readable when
several agents work it:

```bash
export CLIBAN_ACTOR=claude       # tags everything cliban records for you
```

**Move the ticket when the work moves.** A board that lags reality is worse
than no board. Attach the reason in the same call:

```bash
cliban issue mv CLI-42 in-progress
cliban issue mv CLI-42 blocked --note "upstream fix needed: rusqlite #1234"
cliban issue mv CLI-42 in-review --note "PR #88, tests green"
cliban issue mv CLI-42 done --note "merged as abc1234"
```

**Log the things a diff won't tell anyone**, with `cliban issue log`:

- when you start, if the approach isn't obvious from the ticket
- what you *found* — the actual root cause, the surprise, the dead end
- decisions and their reasons, especially ones you'd otherwise re-litigate
- anything you had to discover the hard way

```bash
cliban issue log CLI-42 "Root cause: position is f64 and collapses after ~50 reorders. Renumbering on write, not read."
cliban issue log CLI-42 "Tried a rebalance-on-read pass first — needs a write lock on every read. Abandoned."
```

Log **facts and reasons, not narration**. "Working on it" and "still going" are
noise. If a future agent wouldn't act differently for having read it, don't
write it.

**Tick plan steps as you finish them** — `cliban issue tick CLI-42 --task 1
--step 2` — so progress is visible without reading the code.

**Promote scope you discover** rather than silently widening the ticket:
`cliban issue promote CLI-42 --task 1 --step 3 --title "..."`, or file a fresh
issue and link it with `--blocked-by` / `--related-to`.

**Put durable lessons in project memory**, not in the ticket. A ticket is
closed and forgotten; `## Notes` on the project is what the next session reads.

**Read the timeline before you start**: `cliban activity --since 3d --json`
and `cliban issue show CLI-42 --section activity` tell you what already
happened, including what a previous agent tried and rejected.

## Command inventory

Everything that exists. `--db PATH` is global; `--json` is available on every
read and most writes.

| Command | Purpose |
|---|---|
| `project add\|ls\|show\|edit\|search\|archive\|unarchive` | projects + project memory |
| `issue add\|ls\|show\|edit\|mv` | the core loop |
| `issue archive\|unarchive\|archive-done` | keep the board clean |
| `issue log\|tick\|promote` | plan + activity-log mechanics (see below) |
| `issue import` | bulk create from NDJSON |
| `issue blocked\|current` | what's stuck / what branch am I on |
| `activity` | **what changed since \<time\>** |
| `milestone add\|ls\|show\|edit` | milestones |
| `label add\|ls\|rm` | labels |
| `fff` | fuzzy-find, prints the selected key |
| `tui` | the interactive board (needs a TTY — not for agents) |

## Vocabulary

- **Statuses**: `backlog` | `in-progress` | `blocked` | `in-review` | `done`
- **Priorities**: `none` | `low` | `medium` | `high` | `urgent`
- **Milestone statuses**: `open` | `completed` | `cancelled` (cancelled is the
  archived state — there is no separate milestone archive flag)
- **Issue keys**: `{PROJECT}-{N}` like `CLI-42`. Project keys are uppercase
  letters/digits, 2-10 chars, starting with a letter.
- **Sub-issues**: depth limited to 2 — a sub-issue cannot have children. Exit
  code 2 if you try to nest a third level.
- **Relations**: `blocks`, `blocked_by` (reverse of `blocks`), `related_to`
  (symmetric).
- **Labels**: free-form per project. Create with `label add`, attach via
  `--label` on `issue add`/`edit`/`import`.
- **Project memory**: durable context lives under `## Notes` in the *project*
  description, one retrievable lesson per `###` subsection.
- **Timestamps are UTC ISO-8601**, always. `today`/`yesterday` mean UTC
  midnight boundaries, not local ones.

## JSON shapes

Stable and agent-facing. Optional refs are `null` (never omitted), so
destructuring is safe:

```json
{
  "key":            "CLI-42",
  "title":          "...",
  "description":    "...",
  "status":         "backlog",
  "priority":       "high",
  "position":       12000.5,
  "archived":       false,
  "milestone":      "v0.1" | null,
  "parent":         "CLI-3" | null,
  "due_date":       "2026-06-01" | null,
  "labels":         ["bug", "ui"],
  "relations":      [{"type": "blocks", "target": "CLI-9"}, {"type": "blocked_by", "target": "CLI-3"}],
  "git_branch_name":"cli-42-fix-column-ordering",
  "created_at":     "2026-...Z",
  "updated_at":     "2026-...Z",
  "completed_at":   "2026-...Z" | (absent when not done)
}
```

Parse NDJSON with `for line in stdout.splitlines(): json.loads(line)` (or `jq -c`).

## Discovery — run these first on a vague task

```bash
cliban project ls --json
cliban activity --since 1d --json          # what changed, newest first
cliban issue ls --status in-progress --json
cliban issue blocked --json                # what's stuck on something
cliban milestone ls --sort activity --stats --json
```

## What changed recently

```bash
cliban activity                                  # last 24h, all projects
cliban activity --since yesterday --json
cliban activity --since 3d --project CLI --json
cliban activity --since 2026-07-20 --limit 200 --json
cliban activity --since 1w --milestone "v0.1" --json
```

Emits a merged, newest-first feed of:

- `created` / `completed` — an issue opened or finished in the window (an
  issue that did both reports both)
- `updated` — any other change, only when `created`/`completed` don't already
  explain it
- `status` — a move: `"backlog → in-progress"`, plus your `--note` if you gave one
- `edit` — what one `issue edit` changed: `"priority: low → urgent, +label bug"`
- `plan` — `"ticked Task 1 Step 2"`, `"promoted Task 1 Step 3 → CLI-18"`
- `archive` — `"archived"` / `"unarchived"`
- `log` — a note you wrote with `issue log`

Everything but the first two bullets is recorded automatically and attributed
to `$CLIBAN_ACTOR`. State events have `message: null`; recorded ones carry the
detail in `message`.

Fields: `ts`, `key`, `project`, `kind`, `issue_status`, `title`, `message`,
`actor`, `milestone`. **`issue_status` is the issue's status *now*, not at the
time of the event** — the transition itself is in `message`. Text output
truncates long messages; `--json` never does. Defaults: `--since 1d`,
`--limit 50` (`--limit 0` for no cap), `--archived` to include archived issues.

Filter by kind with jq: `cliban activity --json | jq -c 'select(.kind=="status")'`.

For "which issues did I touch" rather than an event feed:

```bash
cliban issue ls --updated-since 2d --json
cliban issue ls --updated-since today --project CLI --status done --json
```

Accepted by both flags: `45s`, `90m`, `4h`, `3d`, `2w`, `today`, `yesterday`,
`2026-07-25`, `2026-07-25T06:30:00Z`.

## Common recipes

### Create a project (KEY is positional)
```bash
cliban project add CLI --name "Cliban" --description "kanban board"
```

### Capture a new issue
```bash
cliban issue add --project CLI \
  --title "Fix the kanban column ordering" \
  --description "When more than 5 cards exist in IN-REVIEW, positions go negative." \
  --priority high --due 2026-06-01 \
  --label bug --label ui \
  --blocked-by CLI-3 --related-to CLI-7 \
  --json
```

### Move work along
```bash
cliban issue mv CLI-12 in-progress
cliban issue mv CLI-12 blocked --note "waiting on upstream fix"
cliban issue mv CLI-12 done          # stamps completed_at
```
Every move is recorded on the issue's timeline automatically (`backlog →
in-progress`, attributed to `$CLIBAN_ACTOR`). `--note` adds the why.

### Read an issue
```bash
cliban issue show CLI-42 --json
cliban issue show CLI-42 --section plan       # just one section: spec|plan|activity|notes
cliban issue current --json                   # the issue for the current git branch
```
`--section` and `issue current` exit **1** when the section (or a branch-matched
issue) doesn't exist. That is a normal "nothing there" answer, not a failure —
handle it rather than retrying.

### Filter and sort
```bash
cliban issue ls --project CLI --status blocked --json
cliban issue ls --project CLI --label bug --json        # ALL-of semantics
cliban issue ls --project CLI --sort priority --json    # urgent first (default desc)
cliban issue ls --project CLI --sort created:asc --json
cliban issue ls --parent CLI-12 --json                  # sub-issues of one parent
cliban issue ls --no-subs --json                        # top-level only
cliban issue ls --search "column ordering" --limit 20 --json
```
`--search` adds a `score` field and respects every other filter. Default limit
is 50 when `--search` is set, uncapped otherwise.

### Bulk-import from NDJSON
```bash
cat <<'EOF' > /tmp/imp.ndjson
{"project":"CLI","title":"alpha","priority":"high","labels":["bug"]}
{"project":"CLI","title":"beta","milestone":"v0.1","blocked_by":"CLI-1"}
EOF
cliban issue import /tmp/imp.ndjson --json
cliban issue import - < /tmp/imp.ndjson --json     # or stream
```
Each line is `{project, title, [description, status, priority, milestone,
parent, labels]}`. With `--project KEY`, records may omit `project`.

### Sub-issues, parents, relations
```bash
cliban issue add --project CLI --parent CLI-12 --title "Repro test" --json
cliban issue edit CLI-12 --clear-parent          # promote back to top level
cliban issue edit CLI-12 --blocks CLI-9
cliban issue edit CLI-12 --blocked-by CLI-3
cliban issue edit CLI-12 --related-to CLI-7
cliban issue edit CLI-12 --remove-relation CLI-9
```

### Multi-line text
```bash
cliban issue add --project CLI --title "Plan" --description-file ./plan.md
cliban issue edit CLI-12 --description - < /tmp/desc.md      # '-' reads stdin
```
On `edit` these **replace** the description — see trap 1. Safe on `add`, where
there is nothing to lose.

### Labels
```bash
cliban label add bug --project CLI
cliban label ls --project CLI --json
cliban issue edit CLI-12 --label bug --remove-label stale
```

### Milestones
```bash
cliban milestone add --project CLI --name "v0.1" --target 2026-06-01
cliban milestone show v0.1 --project CLI --with-issues --json   # positional NAME
cliban issue edit CLI-12 --milestone "v0.1"
cliban issue edit CLI-12 --clear-milestone

cliban milestone ls --sort activity --stats --json   # all projects, recent first
cliban milestone ls --project CLI --status open --sort target --json
cliban milestone edit --project CLI --name v0.1 --status completed
```
`--sort` is `activity` (most recently worked on) | `name` (default) | `target`
(soonest first, undated last). `--stats` adds done/total and last-activity
columns; in `--json`, `done_count`, `last_activity`, `last_activity_human`.

### Archiving — there is no delete
```bash
cliban issue archive CLI-12
cliban issue unarchive CLI-12
cliban issue ls --project CLI --archived --json      # archived are excluded by default
cliban issue archive-done --project CLI --json       # sweep the done column
cliban project edit CLI --auto-archive-done-after 7d # then:
cliban issue archive-done --auto --json              # honors each project's policy

cliban project archive CLI                           # same for projects
cliban milestone edit --project CLI --name v0.1 --status cancelled
```
`--auto-archive-done-after 0` disables the policy.

**Nothing is ever deleted.** Deleting a row would take its timeline with it,
and a history with holes is worse than no history. `issue rm` and `project rm`
therefore *archive*, and `milestone rm` *cancels* — each succeeds, reports what
it actually did, and names the undo:

```bash
$ cliban issue rm CLI-12
archived CLI-12 — cliban archives instead of deleting (undo: cliban issue unarchive CLI-12)
```

Prefer the real command (`issue archive`) when you know what you want;
`rm` is there so reaching for it by habit still does the right thing.
Archiving is reversible: the issue keeps its key, relations and recorded past,
and simply stops appearing in default lists.

(`label rm` genuinely deletes — a label is a tag, not a work item. It has no
timeline, and detaching it destroys nothing.)

## The description contract

`issue log`, `issue tick`, `issue promote` and `issue show --section` parse the
markdown structure of an issue's `description`. Four H2 anchors are reserved
and matched **exactly**; anything else in the description is left untouched.

- `## Spec` — the design/brainstorm output
- `## Plan` — the implementation plan
- `## Activity Log` — chronological events (what `activity` reads back)
- `## Notes` — long-lived notes (mainly on *project* descriptions)

Plan tasks are numbered H3 headings; steps are GFM checkboxes at column zero
(indented child bullets are not steps):

```markdown
## Plan

### Task 1: short title

- [ ] **Step 1: ...**
- [ ] **Step 2: ...**
```

```bash
cliban issue log CLI-42 "rebased onto main, tests green"
cliban issue log CLI-42 --message-file - < /tmp/note.md
cliban issue tick CLI-42 --task 1 --step 2                  # - [ ] -> - [x]
cliban issue promote CLI-42 --task 1 --step 3 \
  --title "CSRF middleware" --as sub-issue                  # or --as related
```

`issue log` writes its line into the description's `## Activity Log` **and**
records it durably, so a later `--description` rewrite cannot erase the note
from the timeline (it will erase the markdown copy — and the timeline will say
so: `"description rewritten, dropped ## Activity Log"`).

`cliban issue show KEY --section activity` and `cliban activity` both show one
merged, chronological history: everything cliban recorded, plus any
hand-written `## Activity Log` lines that have no record behind them. Entries
appear once, never twice.

`promote` splits a step into its own issue and suffixes the step with `→ KEY`.
If the structure is violated (no `## Plan`, renamed `### Task N`, step already
ticked), these exit 2 with a message naming the structural problem — there is
no best-effort recovery. Fix the description and retry.

**Prefer `issue log` over rewriting the description** to record progress: it
appends atomically, keeps the timestamp format `activity` can read, and won't
clobber a concurrent edit.

## Persistent agent memory

Durable knowledge that does not belong to any one issue goes under `## Notes`
in the *project* description, one lesson per `###` subsection.

```bash
cliban project search CLI "sqlite canonical" --json      # search first
cliban project search CLI "wal" --section all --limit 5 --json
cliban project show CLI --section notes
cliban project edit CLI --description-file - < updated-project.md
```

`search` fuzzy-matches every whitespace-separated term against each heading and
body, returns only matching subsections, ranked, capped by `--limit` (default
20). Emits NDJSON of `{project, heading, content, score}`. Do not load the
whole notes section unless the task genuinely needs it.

## Exit codes

- `0` success
- `1` not found
- `2` validation error (bad status, depth-2 violation, missing required flag,
  unparseable `--since`, broken plan structure)
- `3` internal/db error

## DB location

`$XDG_DATA_HOME/cliban/cliban.db`, falling back to
`~/.local/share/cliban/cliban.db`. Override with `--db <path>` or `$CLIBAN_DB`.

## What NOT to do

- Don't parse the table output of `ls`/`show`. Use `--json`.
- Don't pass `--editor`/`-e` without a TTY — exit code 2. Without it, `add`
  requires `--title` and `edit` requires at least one mutation flag. (Legacy
  `--no-editor` is accepted as a no-op.)
- Don't nest sub-issues three levels deep.
- Don't hand-filter archived issues; pass `--archived` to include them.
- Don't assume local time — everything is UTC.
- Don't create placeholder backlog/done issues as agent memory. Use `## Notes`
  and `project search`.
- Don't finish work without moving the ticket, and don't move it without a
  `--note` when the reason isn't obvious from the title.
- Don't log narration ("working on it"). Log findings, decisions and dead ends.
- Don't rewrite a whole description just to note progress — `--description`
  wipes the Activity Log. Use `issue log` / `issue tick`.
- Don't expect `rm` to delete: it archives (milestones: cancels) and tells you
  so. Nothing in cliban destroys a work item.
- Don't invent flags: there is no `--key`, no `issue move`, no `--all`. If a
  flag is rejected, run `cliban <cmd> --help` rather than guessing again.
