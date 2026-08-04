---
name: cliban
description: Drive the local cliban kanban board via its CLI. Use when the user mentions cliban, kanban, ticket, issue, project, milestone, or asks you to capture/move work items, or asks what changed/what happened on the board.
---

# Using cliban

A terminal-first kanban board built for agents. One grammar covers the whole
surface:

    cliban <noun> <verb> [IDENTITY] [flags]

- **Nouns**: `project`, `issue`, `milestone`, `label`, `linear`, `activity`
  (the event feed), `tui` (humans only).
- **Identity is positional**: `issue show PROJ-42`, `issue add "Fix
  ordering"`, `milestone edit "v0.1" --status completed`. Milestones are
  addressed by name (project-scoped); issues by key. A flag-looking title
  needs the standard `--` escape.
- **Filters are short flags**: `-p` project KEY, `-s` status, `-m` milestone.
- **Ambient scope**: `$CLIBAN_PROJECT` (set per-repo via direnv) is the
  default `-p` everywhere. Explicit `-p KEY` beats it; `-p '*'` deliberately
  widens to every project. Commands that need a project error with both
  spellings when neither is set. It also stands in for the positional KEY on
  project reads and memory appends (`project show|cat|search|note add`) —
  never on structural writes (`edit`, `archive`).
- **Three viewers, one job each**: `ls` = many lean rows; `show` = one
  complete entity; `cat` = raw markdown bytes (whole description or
  `--section X`), never formatted.
- **Output follows the reader**: piped stdout → JSON/NDJSON, TTY → tables;
  `--json`/`--table` force it, `$CLIBAN_OUTPUT` pins it. Mutations echo the
  changed entity as one compact lean JSON line when piped, confirm in one
  line on a TTY. `cat` is the exception: always bytes, every mode.

## Traps

1. **`--description`/`--description-file` on `edit` REPLACES the whole
   description**, destroying `## Plan`, `## Activity Log`, everything. Use
   the narrow tools: `edit --section spec|plan|notes` (one section),
   `issue log` (append progress), `issue tick` (tick a step),
   `project note add` (lesson).
2. **Statuses are fixed**: `backlog | in-progress | blocked | in-review |
   done`. Move with `issue mv KEY done` — the verb is `mv`.
3. **Time args share one parser** (`--since`, `--updated-since`): `45s`,
   `4h`, `3d`, `2w`, `today`, `yesterday`, `2026-07-25`, RFC3339. All UTC.

## Protocol

cliban records *what* changed on its own — every move, edit, tick, and log
lands on the issue's timeline, attributed to `$CLIBAN_ACTOR` when set, else
the ambient Claude session (`session:<first-8>`). You supply the **why**:

```bash
cliban issue claim PROJ-42            # before touching shared work (release / claim --force exist)
cliban issue mv PROJ-42 in-progress
cliban issue log PROJ-42 "Root cause: f64 positions collapse after ~50 reorders"
cliban issue tick PROJ-42 --task 1 --step 2   # --task optional when the plan has one task
cliban issue mv PROJ-42 done --note "merged as abc1234"
```

- Move the ticket when the work moves; attach the reason with `--note`.
- Log **findings, decisions, dead ends** — never narration ("working on it").
- Promote discovered scope instead of widening the ticket:
  `cliban issue promote PROJ-42 --task 1 --step 3` (title defaults to the
  step's own text; `--title` overrides; `--as sub-issue|related`), or file a
  new issue with `--blocked-by`.
- Before starting: `cliban activity --since 3d` (the board lately) and
  `cliban activity --issue PROJ-42` (one ticket's whole merged history) —
  including approaches already tried and rejected.
- **Retry-safe:** re-running a mutation whose state already holds is a no-op
  that says so (`"noop": true` in JSON) and writes nothing. Retry freely
  after timeouts; only genuinely wrong targets fail.

## Command map

`--db PATH` is global; `$CLIBAN_DB` and `$CLIBAN_PROJECT` come from the
environment.

| Command | Purpose |
|---|---|
| `project add\|ls\|show\|cat\|edit\|search\|archive\|unarchive` | projects + project memory |
| `project note add` | append one `###` lesson under project `## Notes` |
| `issue add\|ls\|show\|cat\|edit\|mv` | the core loop |
| `issue log\|tick\|promote` | plan + activity mechanics |
| `issue append-section` | atomic append to the end of one H2 section |
| `issue lint` | validate description structure before `tick` bites |
| `issue claim\|release` | session ownership on a shared board |
| `issue current` | the issue for the current git branch |
| `issue cp` | copy an issue's shape (plan reset) — never its history |
| `issue import` | bulk create from NDJSON |
| `issue archive\|unarchive\|archive-done` | keep the board clean |
| `activity` | what changed since \<time\>; `--issue KEY` = one full history |
| `milestone add\|ls\|show\|edit\|waves` | milestones (`waves` = dependency order) |
| `label add\|ls\|rm` | labels (`label rm` deletes — labels have no history) |
| `linear import\|push\|sync` | Linear bridge — see references/linear-bridge.md |

There is no delete: archiving is the terminal state, reversible, and keeps
the key, relations, and recorded past.

## Vocabulary

- Priorities: `none|low|medium|high|urgent`. Milestone statuses:
  `open|completed|cancelled` (cancelled = archived; no separate flag).
- Keys are `PROJ-42`; project keys uppercase, 2–10 chars. Sub-issue depth
  max 2. Relations: `blocks`, `blocked_by`, `related_to`.

## JSON shapes

**List rows are lean; single-entity output is complete.**

- **Lean** (`issue ls`, `project ls`, `milestone ls`, and every mutation
  echo): a field that is null, empty, or the default is **absent** — no
  `"milestone":null`, no `"labels":[]`, no `"archived":false`. Never
  present: `description`, `git_branch_name`, `position`, `created_at`.
  Second-precision timestamps, except a mutation echo's `updated_at`, which
  keeps microsecond precision — **the echo is a valid `--if-updated-at` CAS
  token**, so you can chain edits without a re-`show`. Echoes also carry
  `"noop":true` on retry no-ops. Read with `.get()` / jq; never destructure
  by fixed keys.
- **Full** (`issue show`, `issue current`, `--full` on any list): all
  fields, optional ones `null`, microsecond timestamps.
  Fields: key, title, description, status, priority, position, archived,
  milestone, parent, due_date, labels, relations `[{type,target}]`,
  git_branch_name, created_at, updated_at, plus completed_at/claimed_by when
  set.

The bodies are the payload: `ls` to find keys, `show` to read one entity,
`--full` only deliberately and never without a project scope.

## Reads

```bash
cliban activity --since 1d                 # merged event feed: created/completed/updated/status/edit/plan/archive/log
cliban activity --issue PROJ-42            # one issue's complete history (all time, no cap)
cliban issue ls -s in-progress
cliban issue ls --ready                    # takeable: backlog, unblocked, unclaimed
cliban issue ls --blocked                  # at least one open blocker
cliban issue ls -m "v0.1" --sort priority  # sort: priority|created|updated|position[:asc|desc]
cliban issue ls --search "ordering" --limit 20   # fuzzy; adds score; --limit caps any ls
cliban issue ls --label bug --no-subs --parent PROJ-12 --updated-since 2d
cliban issue show PROJ-42
cliban issue cat PROJ-42                   # whole description, verbatim bytes
cliban issue cat PROJ-42 --section plan    # one section's body: spec|plan|activity|notes or any verbatim H2
cliban issue current                       # branch-matched issue; exit 1 = none (an answer, not a failure)
cliban milestone ls                        # in scope: lean rows; unscoped: per-project counts only
cliban milestone ls --stats                # + done_count/last_activity (needs a project scope)
cliban milestone show "v0.1"
cliban milestone waves "v0.1"              # {"waves":[[...],[...]],"done":[...],"external_blocked":[...]}
```

All of these assume `$CLIBAN_PROJECT`; add `-p KEY` to point elsewhere,
`-p '*'` to span every project. `--archived` includes archived issues.
`activity` fields: `ts,key,project,kind,title` + `message/actor/milestone`
when set. Wave N of `waves` is dispatchable once waves 1..N-1 are done;
`external_blocked` is gated by open work outside the milestone.

## Writes

```bash
cliban issue add "Fix the column ordering" --description "..." --priority high \
  --label bug --label ui -m "v0.1" --due 2026-06-01 -s backlog \
  --parent PROJ-12 --blocked-by PROJ-3 --related-to PROJ-7   # everything but the title optional
cliban issue edit PROJ-12 --priority urgent --label perf --remove-label stale
cliban issue edit PROJ-12 -m "v0.1"               # or --clear-milestone / --clear-parent
cliban issue edit PROJ-12 --blocks PROJ-9         # or --blocked-by / --related-to / --remove-relation
cliban issue edit PROJ-12 --title "Renamed"
cliban issue cp PROJ-12 --title "Q3 edition"      # copies title/Spec/Plan(reset)/Notes/labels/priority
cliban issue import ./items.ndjson                # lines: {project,title,[description,status,priority,milestone,parent,labels]}; '-' = stdin
cliban label add bug
cliban milestone add "v0.1" --target 2026-06-01
cliban milestone edit "v0.1" --status completed   # or --name (rename) / --target / --clear-target
cliban project add PROJ "Cliban"                  # display name optional (default: the key)
```

Multi-line text: `--description-file ./x.md`, or `-` for stdin. Safe on
`add`; on `edit` it replaces everything (trap 1).

## The description contract

Four H2 anchors are reserved, matched exactly: `## Spec`, `## Plan`,
`## Activity Log`, `## Notes`. Other H2s are yours; section tools address
them by verbatim anchor. Plan tasks are `### Task N:` headings; steps are
column-zero GFM checkboxes (`- [ ] **Step 1: ...**`).

```bash
cliban issue edit PROJ-12 --section spec --description-file -   # replace ONE section, others byte-identical
cliban issue edit PROJ-12 --section "Open questions" --create-section --description='- who owns auth?'
cliban issue append-section PROJ-12 --section notes "- block appended to the END of the section"
cliban issue lint PROJ-12               # exit 2 + findings when tick would choke
cliban issue log PROJ-12 "note"         # writes ## Activity Log AND the durable record
```

- Writing to a section that doesn't exist: exit 2 listing the ones that do;
  `--create-section` when you mean to add one. Payload is the section *body*
  (a leading same-anchor H2 line is stripped; any other H2 inside = exit 2).
- `--section activity` is refused on writes — the log belongs to `issue log`.
- Structure violations (`no ## Plan`, renamed task, already-ticked step) exit
  2 naming the problem; fix the description, don't retry blind.
- `issue log` / `append-section` / `project note add` all read piped stdin
  when the text argument is absent.
- Racy round-trips: pin the read —
  `TS=$(cliban issue show PROJ-12 --json | jq -r .updated_at)` then
  `cliban issue edit PROJ-12 ... --if-updated-at "$TS"` (exit 2 = stale;
  re-read and retry). `project edit` takes the same flag. CAS timestamps
  come from `show` or a mutation echo's `updated_at` — never from list rows.

## Project memory

Durable lessons live under `## Notes` on the *project*, one `###` per lesson
— not in tickets, not in placeholder issues.

```bash
cliban project search "sqlite wal" --limit 5   # NDJSON {project,heading,content,score}; search before loading
cliban project cat --section notes             # explicit KEY first positional addresses another project
cliban project note add "cargo test needs --test-threads=1" --body - <<'EOF'
Fixtures share a tempdir; parallel runs corrupt it.
EOF
```

## Archiving

```bash
cliban issue archive PROJ-12              # reversible: unarchive; key/history kept
cliban issue archive-done                 # sweep the done column; --auto honors project policy
cliban project edit PROJ --auto-archive-done-after 7d   # 0 disables
cliban project archive KEY
cliban milestone edit "v0.1" --status cancelled
```

## Exit codes

`0` ok · `1` not found (also: no section, no branch-matched issue) ·
`2` validation (bad status, bad flag, missing scope, broken plan structure,
stale CAS) · `3` internal/db error.

DB: `$XDG_DATA_HOME/cliban/cliban.db` (fallback `~/.local/share/...`);
override `--db` / `$CLIBAN_DB`.

## Don'ts

- Don't parse table output — piped output is already JSON.
- Don't pass `--editor`/`-e` without a TTY (exit 2).
- Don't note progress by rewriting descriptions (trap 1); don't log narration.
- Don't create placeholder issues as memory — `project note add` + `project search`.
- Don't start shared work unclaimed; don't finish without moving the ticket.
- Don't invent flags or verbs — on rejection run `cliban <cmd> --help` once
  instead of guessing again.
