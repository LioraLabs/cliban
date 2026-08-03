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
   Almost always you want a narrower tool: `issue edit KEY --section
   spec|plan|notes --description-file -` replaces ONE section and leaves the
   rest byte-identical; `issue log` appends progress; `issue tick` ticks a
   step; `project note add` appends a project lesson. A bare `--description`
   on `edit` is for genuinely starting over. The timeline records the
   destruction (`"description rewritten, dropped ## Plan"`) and logged notes
   survive it, but the markdown is gone.
2. **Always pass `--json` for reads.** The table format is for humans and will
   change. `ls` emits NDJSON (one compact object per line), `show` emits one
   pretty object.
3. **`--project` takes the KEY, not the name**: `--project PROJ`, not
   `--project Cliban`. Keys are uppercase; the CLI upcases what you pass.
4. **`project add` takes the key positionally**: `cliban project add PROJ --name
   "Cliban"`. There is no `--key` flag. (`issue add` is the opposite — it needs
   `--project PROJ --title "..."`.)
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

**Attribution is automatic.** Every entry cliban records is attributed to
`$CLIBAN_ACTOR` when set, else to the ambient Claude Code session
(`session:<first-8>` of `$CLAUDE_CODE_SESSION_ID`) — so concurrent agent
sessions are distinguishable with zero setup. Export `CLIBAN_ACTOR` only when
you want a human-meaningful name instead:

```bash
export CLIBAN_ACTOR=claude       # optional: override the session-id default
```

**Claim what you take.** On a board several sessions share, a claim marks a
ticket as yours before the first status move lands, and `issue ready` stops
offering it to others:

```bash
cliban issue claim PROJ-42            # claims as the resolved actor
cliban issue release PROJ-42          # when you stop without finishing
cliban issue claim PROJ-42 --force    # take over a dead session's claim
```

**Move the ticket when the work moves.** A board that lags reality is worse
than no board. Attach the reason in the same call:

```bash
cliban issue mv PROJ-42 in-progress
cliban issue mv PROJ-42 blocked --note "upstream fix needed: rusqlite #1234"
cliban issue mv PROJ-42 in-review --note "PR #88, tests green"
cliban issue mv PROJ-42 done --note "merged as abc1234"
```

**Log the things a diff won't tell anyone**, with `cliban issue log`:

- when you start, if the approach isn't obvious from the ticket
- what you *found* — the actual root cause, the surprise, the dead end
- decisions and their reasons, especially ones you'd otherwise re-litigate
- anything you had to discover the hard way

```bash
cliban issue log PROJ-42 "Root cause: position is f64 and collapses after ~50 reorders. Renumbering on write, not read."
cliban issue log PROJ-42 "Tried a rebalance-on-read pass first — needs a write lock on every read. Abandoned."
```

Log **facts and reasons, not narration**. "Working on it" and "still going" are
noise. If a future agent wouldn't act differently for having read it, don't
write it.

**Tick plan steps as you finish them** — `cliban issue tick PROJ-42 --task 1
--step 2` — so progress is visible without reading the code.

**Promote scope you discover** rather than silently widening the ticket:
`cliban issue promote PROJ-42 --task 1 --step 3 --title "..."`, or file a fresh
issue and link it with `--blocked-by` / `--related-to`.

**Put durable lessons in project memory**, not in the ticket. A ticket is
closed and forgotten; `## Notes` on the project is what the next session reads.

**Read the timeline before you start**: `cliban activity --since 3d --json`
and `cliban issue show PROJ-42 --section activity` tell you what already
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
| `issue append-section` | atomic append to the end of one H2 section |
| `issue lint` | validate the description contract before tick bites |
| `issue import` | bulk create from NDJSON |
| `issue blocked\|ready` | what's stuck / **what can I take right now** |
| `issue claim\|release` | session-scoped ownership on a shared board |
| `issue current` | what branch am I on |
| `activity` | **what changed since \<time\>** |
| `project note add` | append one `###` lesson under project `## Notes` |
| `milestone add\|ls\|show\|edit` | milestones |
| `milestone waves` | dependency-wave partition for orchestration |
| `label add\|ls\|rm` | labels |
| `fff` | fuzzy-find, prints the selected key |
| `import linear` | pull a Linear issue onto the board (see below) |
| `push linear` | send state + progress back to Linear (see below) |
| `tui` | the interactive board (needs a TTY — not for agents) |

## Vocabulary

- **Statuses**: `backlog` | `in-progress` | `blocked` | `in-review` | `done`
- **Priorities**: `none` | `low` | `medium` | `high` | `urgent`
- **Milestone statuses**: `open` | `completed` | `cancelled` (cancelled is the
  archived state — there is no separate milestone archive flag)
- **Issue keys**: `{PROJECT}-{N}` like `PROJ-42`. Project keys are uppercase
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

**`description` is a `show` field, not an `ls` field.** Listing commands omit
the body; `--full` restores it. That is not a nicety: on a real board the
bodies ARE the payload — `issue ls --project COOK --json` was 2.27 MB, 95% of
it descriptions, against 119 KB without them. Reach for `ls` to find keys and
`show` to read one issue. Pass `--full` only when you genuinely need every body
at once, and never without `--project`.

Lean by default: `issue ls`, `issue blocked`, `milestone ls`, `fff`, and the
issues nested in `milestone show --with-issues`.
Always full: `issue show`, `issue current`, `milestone show`'s own body, the
JSON echoed by `add` / `edit` / `mv` / `import`.

```json
{
  "key":            "PROJ-42",
  "title":          "...",
  "description":    "...",     // ls: omitted unless --full; show: always
  "status":         "backlog",
  "priority":       "high",
  "position":       12000.5,
  "archived":       false,
  "milestone":      "v0.1" | null,
  "parent":         "PROJ-3" | null,
  "due_date":       "2026-06-01" | null,
  "labels":         ["bug", "ui"],
  "relations":      [{"type": "blocks", "target": "PROJ-9"}, {"type": "blocked_by", "target": "PROJ-3"}],
  "git_branch_name":"proj-42-fix-column-ordering",
  "created_at":     "2026-...Z",
  "updated_at":     "2026-...Z",
  "completed_at":   "2026-...Z" | (absent when not done),
  "claimed_by":     "session:ea8a9c5e" | (absent when unclaimed)
}
```

Parse NDJSON with `for line in stdout.splitlines(): json.loads(line)` (or `jq -c`).

## Discovery — run these first on a vague task

```bash
cliban project ls --json
cliban activity --since 1d --json          # what changed, newest first
cliban issue ls --status in-progress --json
cliban issue blocked --json                # what's stuck on something
cliban issue ready --json                  # what's takeable right now
cliban milestone ls --sort activity --stats --json
```

Scope reads with `--project KEY` once you know which board you are on. An
unfiltered `ls` walks every project on the machine.

### Finding a milestone and its work

Two calls, not five. `milestone ls` gives you the exact names (needed because
milestones are addressed by name, not a key); `milestone show` gives you the
one you want.

```bash
cliban milestone ls --project COOK --json | jq -c '{name, status}'
cliban milestone show "code-path unification" --project COOK --json | jq -r '.description'
cliban issue ls --project COOK --milestone "code-path unification" --json \
  | jq -c '{key, title, status, priority}'
```

Do NOT read a milestone's spec out of `milestone ls` — it is not there, and it
was never the right call even when it was: you would be fetching every
milestone on the board to read one.

## What changed recently

```bash
cliban activity                                  # last 24h, all projects
cliban activity --since yesterday --json
cliban activity --since 3d --project PROJ --json
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
- `plan` — `"ticked Task 1 Step 2"`, `"promoted Task 1 Step 3 → PROJ-18"`
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
cliban issue ls --updated-since today --project PROJ --status done --json
```

Accepted by both flags: `45s`, `90m`, `4h`, `3d`, `2w`, `today`, `yesterday`,
`2026-07-25`, `2026-07-25T06:30:00Z`.

## Common recipes

### Create a project (KEY is positional)
```bash
cliban project add PROJ --name "Cliban" --description "kanban board"
```

### Capture a new issue
```bash
cliban issue add --project PROJ \
  --title "Fix the kanban column ordering" \
  --description "When more than 5 cards exist in IN-REVIEW, positions go negative." \
  --priority high --due 2026-06-01 \
  --label bug --label ui \
  --blocked-by PROJ-3 --related-to PROJ-7 \
  --json
```
`add` also accepts `--status` and `--milestone "NAME"` — set them at creation
instead of a follow-up `mv`/`edit`. Status defaults to `backlog`.

### Move work along
```bash
cliban issue mv PROJ-12 in-progress
cliban issue mv PROJ-12 blocked --note "waiting on upstream fix"
cliban issue mv PROJ-12 done          # stamps completed_at
```
Every move is recorded on the issue's timeline automatically (`backlog →
in-progress`, attributed to `$CLIBAN_ACTOR`). `--note` adds the why.

### Read an issue
```bash
cliban issue show PROJ-42 --json
cliban issue show PROJ-42 --section plan       # just one section: spec|plan|activity|notes
cliban issue current --json                   # the issue for the current git branch
```
`--section` and `issue current` exit **1** when the section (or a branch-matched
issue) doesn't exist. That is a normal "nothing there" answer, not a failure —
handle it rather than retrying.

### Filter and sort
```bash
cliban issue ls --project PROJ --status blocked --json
cliban issue ls --project PROJ --label bug --json        # ALL-of semantics
cliban issue ls --project PROJ --sort priority --json    # urgent first (default desc)
cliban issue ls --project PROJ --sort created:asc --json
cliban issue ls --parent PROJ-12 --json                  # sub-issues of one parent
cliban issue ls --no-subs --json                        # top-level only
cliban issue ls --search "column ordering" --limit 20 --json
```
`--search` adds a `score` field and respects every other filter. Default limit
is 50 when `--search` is set, uncapped otherwise.

### Bulk-import from NDJSON
```bash
cat <<'EOF' > /tmp/imp.ndjson
{"project":"CLI","title":"alpha","priority":"high","labels":["bug"]}
{"project":"CLI","title":"beta","milestone":"v0.1","blocked_by":"PROJ-1"}
EOF
cliban issue import /tmp/imp.ndjson --json
cliban issue import - < /tmp/imp.ndjson --json     # or stream
```
Each line is `{project, title, [description, status, priority, milestone,
parent, labels]}`. With `--project KEY`, records may omit `project`.

### The frontier — what can I take right now
`ready` is the complement of `blocked`: backlog status, not archived, every
blocker done, nobody's claim on it. Claim before you start so concurrent
sessions skip it:
```bash
cliban issue ready --project PROJ --json
cliban issue ready --parent PROJ-12 --json               # decision tickets of a map
cliban issue ready --project PROJ --milestone "v0.1" --json
cliban issue claim PROJ-42 && cliban issue mv PROJ-42 in-progress
```

### Milestone waves — what runs in parallel, in what order
```bash
cliban milestone waves --project PROJ "v0.1" --json
# {"waves":[["PROJ-1","PROJ-4"],["PROJ-2"],["PROJ-3"]],
#  "done":[...], "external_blocked":[...]}
```
Wave N is safe to dispatch once waves 1..N-1 are done. `external_blocked`
lists issues gated by open work *outside* the milestone — finishing the waves
won't unblock them. A dependency cycle exits 2 naming the issues in it.

### Sub-issues, parents, relations
```bash
cliban issue add --project PROJ --parent PROJ-12 --title "Repro test" --json
cliban issue edit PROJ-12 --clear-parent          # promote back to top level
cliban issue edit PROJ-12 --blocks PROJ-9
cliban issue edit PROJ-12 --blocked-by PROJ-3
cliban issue edit PROJ-12 --related-to PROJ-7
cliban issue edit PROJ-12 --remove-relation PROJ-9
```

### Multi-line text
```bash
cliban issue add --project PROJ --title "Plan" --description-file ./plan.md
cliban issue edit PROJ-12 --description - < /tmp/desc.md      # '-' reads stdin
cliban issue add --project PROJ --title "..." --description-file - --json <<'EOF'
## Spec
...
EOF
```
On `edit` these **replace** the description — see trap 1. Safe on `add`, where
there is nothing to lose.

### Write one section without touching the rest
`--section` replaces exactly one H2 and leaves every other byte alone —
atomic on the store thread, no read-modify-write:
```bash
cliban issue edit PROJ-12 --section spec --description-file - <<'EOF'
The spec text.
EOF
cliban issue edit PROJ-12 --section plan --description-file ./plan.md
cliban issue edit PROJ-12 --section "Open questions" --create-section --description='- who owns auth?'
cliban issue show PROJ-12 --section "Open questions"
```
`spec|plan|activity|notes` are aliases for the contract sections; **any other
value is a verbatim H2 anchor** — `--section "Decisions so far"` targets
`## Decisions so far`, exact match. Writing to a section that doesn't exist is
exit 2 listing the sections that do (so a typo can't silently append a junk
section); pass `--create-section` when you genuinely mean to add one. Payloads
carry the section *body* only: a leading `## <same anchor>` line is stripped
for you, and any other H2 inside the payload is exit 2 (it would terminate
the section and silently orphan what follows).
`--section activity` is refused on writes — the Activity Log belongs to
`issue log`. After writing a plan by hand, check it parses:
```bash
cliban issue lint PROJ-12        # exit 2 + findings when tick would choke
```

### Append to a section without rewriting it
`append-section` adds a block to the END of one section, atomically — the
tool for growing a list or journal-style section. Leading `-` works, so
markdown bullets need no quoting tricks:
```bash
cliban issue append-section PROJ-12 --section "Decisions so far" "- picked sqlite over pg — single-writer fits"
echo "long block" | cliban issue append-section PROJ-12 --section notes --text-file -
```
Same existence policy and `--create-section` escape hatch as `edit --section`;
activity refused (use `issue log`, which stamps and dedupes).

### Guard racy edits with compare-and-swap
Concurrent sessions share this board. When you round-trip a description (or
any edit where you decided based on what you read), pin the read:
```bash
TS=$(cliban issue show PROJ-12 --json | jq -r .updated_at)
cliban issue edit PROJ-12 --description-file /tmp/d.md --if-updated-at "$TS"
# exit 2 "stale write" if anything changed since — re-read and retry
```
`project edit` takes the same flag.

### Labels
```bash
cliban label add bug --project PROJ
cliban label ls --project PROJ --json
cliban issue edit PROJ-12 --label bug --remove-label stale
```

### Milestones
```bash
cliban milestone add --project PROJ --name "v0.1" --target 2026-06-01
cliban milestone show v0.1 --project PROJ --with-issues --json   # positional NAME
cliban issue edit PROJ-12 --milestone "v0.1"
cliban issue edit PROJ-12 --clear-milestone

cliban milestone ls --sort activity --stats --json   # all projects, recent first
cliban milestone ls --project PROJ --status open --sort target --json
cliban milestone edit --project PROJ --name v0.1 --status completed
cliban milestone edit --project PROJ --name v0.1 --rename "v0.1.0"
```
Milestones are addressed **by name, not a key** — and only `show` accepts the
name positionally. `add`, `edit`, and `rm` require `--name`; a positional name
there fails with `error: unexpected argument`. When in doubt, pass `--name`.
`--sort` is `activity` (most recently worked on) | `name` (default) | `target`
(soonest first, undated last). `--stats` adds done/total and last-activity
columns; in `--json`, `done_count`, `last_activity`, `last_activity_human`.

### Archiving — there is no delete
```bash
cliban issue archive PROJ-12
cliban issue unarchive PROJ-12
cliban issue ls --project PROJ --archived --json      # archived are excluded by default
cliban issue archive-done --project PROJ --json       # sweep the done column
cliban project edit PROJ --auto-archive-done-after 7d # then:
cliban issue archive-done --auto --json              # honors each project's policy

cliban project archive CLI                           # same for projects
cliban milestone edit --project PROJ --name v0.1 --status cancelled
```
`--auto-archive-done-after 0` disables the policy.

**Nothing is ever deleted.** Deleting a row would take its timeline with it,
and a history with holes is worse than no history. `issue rm` and `project rm`
therefore *archive*, and `milestone rm` *cancels* — each succeeds, reports what
it actually did, and names the undo:

```bash
$ cliban issue rm PROJ-12
archived PROJ-12 — cliban archives instead of deleting (undo: cliban issue unarchive PROJ-12)
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

Descriptions may carry any other H2 you like (`## Rollout`, `## Decisions so
far`); the contract tools ignore them, and `edit --section` /
`append-section` / `show --section` address them by verbatim anchor.

Plan tasks are numbered H3 headings; steps are GFM checkboxes at column zero
(indented child bullets are not steps):

```markdown
## Plan

### Task 1: short title

- [ ] **Step 1: ...**
- [ ] **Step 2: ...**
```

```bash
cliban issue log PROJ-42 "rebased onto main, tests green"
cliban issue log PROJ-42 --message-file - < /tmp/note.md
cliban issue tick PROJ-42 --task 1 --step 2                  # - [ ] -> - [x]
cliban issue promote PROJ-42 --task 1 --step 3 \
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
cliban project search PROJ "sqlite canonical" --json      # search first
cliban project search PROJ "wal" --section all --limit 5 --json
cliban project show PROJ --section notes
cliban project note add PROJ --title "cargo test needs --test-threads=1" --body - <<'EOF'
The fixtures share a tempdir; parallel runs corrupt it and the failures look
like flaky assertions, not contention.
EOF
```

`note add` appends one `### <title>` subsection under `## Notes` and touches
nothing else — always prefer it over `project edit --description-file` for
recording a lesson; the whole-description path is only for restructuring.

`search` fuzzy-matches every whitespace-separated term against each heading and
body, returns only matching subsections, ranked, capped by `--limit` (default
20). Emits NDJSON of `{project, heading, content, score}`. Do not load the
whole notes section unless the task genuinely needs it.

## Linear bridge

Two explicit verbs. Nothing syncs in the background, so nothing crosses the
boundary unless you ask.

```bash
cliban import linear ENG-412 --project PROJ            # pull it onto the board
cliban import linear ENG-412 --project PROJ --dry-run  # see it first
cliban push linear PROJ-42                             # state + progress comment
cliban push linear PROJ-42 --description               # also mirror into the description
cliban push linear PROJ-42 --create --team ENG         # no counterpart yet? make one
```

Needs `$LINEAR_API_KEY`. Optional `~/.config/cliban/linear.toml` sets the
default team and any state-name overrides; never put the token in it.

**Who owns what.** This is the rule that makes the bridge safe to re-run:

| Field | Owner |
|---|---|
| title, priority, labels, due date, workflow state | Linear — a re-import overwrites your local edits |
| `## Spec` | follows who created the pairing: link born from `import` → Linear owns it (re-import refreshes it); link born from `push --create` → cliban owns it (re-import leaves it alone; `push --description` may always mirror it outward) |
| `## Plan`, `## Activity Log`, `## Notes` | cliban — a re-import never touches them |
| Linear description outside cliban's fenced block, Linear comments | humans — never modified |

So: re-import as often as you like, your ticked plan survives. But on an
imported issue, don't edit the title or spec locally and expect it to stick —
change it in Linear. An issue you pushed out with `--create` keeps its local
spec forever.

**Statuses.** `backlog` / `in-progress` / `done` round-trip cleanly.
`blocked` and `in-review` only survive if the Linear team has a column named for
them (Linear types both as "started"), otherwise they collapse into
in-progress. A cancelled Linear issue arrives as `done` **and archived**.

**Gotchas.**

- `push` on an unlinked issue exits 1. Either `--create`, or adopt an existing
  pairing with `import linear ENG-412 --project PROJ --link-to PROJ-42`.
- `push` exits 2 if Linear changed since your last sync. Re-import first, or
  `--force` if you know you are the authority.
- One local issue per Linear issue. A second import refreshes rather than
  duplicating.

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
  wipes the Activity Log. Use `issue log` / `issue tick`, and `--section` for
  spec/plan writes.
- Don't record a project lesson by round-tripping the whole description — use
  `project note add`.
- Don't start work another session may also see without `issue claim`.
- Don't expect `rm` to delete: it archives (milestones: cancels) and tells you
  so. Nothing in cliban destroys a work item.
- Don't invent flags: there is no `--key`, no `issue move`, no `--all`. If a
  flag is rejected, run `cliban <cmd> --help` rather than guessing again.
