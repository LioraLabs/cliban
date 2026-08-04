---
name: cliban
description: Drive the local cliban kanban board via its CLI. Use when the user mentions cliban, kanban, ticket, issue, project, milestone, or asks you to capture/move work items, or asks what changed/what happened on the board.
---

# Using cliban

A terminal-first kanban board built for agents: every read has a `--json`
form, nothing opens an editor unless asked, and mutations are retry-safe.

## Traps — read first

1. **`--description` / `--description-file` on `edit` REPLACES the whole
   description**, destroying `## Plan`, `## Activity Log`, everything. Use the
   narrow tools: `edit --section spec|plan|notes` (one section), `issue log`
   (append progress), `issue tick` (tick a step), `project note add` (lesson).
2. **Piped stdout is already JSON** (NDJSON from lists, one pretty object from
   `show`); a TTY gets tables. `--json`/`--table` force it, `CLIBAN_OUTPUT`
   pins it. Mutations echo the changed entity as JSON when piped.
   (`show --section` is the exception: always raw markdown.)
3. **`--project` / `-p` takes the KEY, not the name** (`-p PROJ`); upcased for
   you.
4. **`project add` takes the key positionally** (`project add PROJ --name
   "..."`); `issue add` instead needs `--project PROJ --title "..."`.
5. **Time args share one parser** (`--since`, `--updated-since`): `45s`, `4h`,
   `3d`, `2w`, `today`, `yesterday`, `2026-07-25`, RFC3339. UTC everywhere.
6. **Statuses are fixed**: `backlog | in-progress | blocked | in-review |
   done`. Move with `issue mv KEY done` — the verb is `mv`, not `move`.

## Protocol

cliban records *what* changed on its own — every move, edit, tick, and log
lands on the issue's timeline, attributed to `$CLIBAN_ACTOR` when set, else
the ambient Claude session (`session:<first-8>`). You supply the **why**:

```bash
cliban issue claim PROJ-42            # before touching shared work (release / claim --force exist)
cliban issue mv PROJ-42 in-progress
cliban issue log PROJ-42 "Root cause: f64 positions collapse after ~50 reorders"
cliban issue tick PROJ-42 --task 1 --step 2
cliban issue mv PROJ-42 done --note "merged as abc1234"
```

- Move the ticket when the work moves; attach the reason with `--note`.
- Log **findings, decisions, dead ends** — never narration ("working on it").
- Promote discovered scope instead of widening the ticket:
  `cliban issue promote PROJ-42 --task 1 --step 3 --title "..."` (`--as
  sub-issue|related`), or file a new issue with `--blocked-by`.
- Before starting: `cliban activity --since 3d -p PROJ` and
  `cliban issue show PROJ-42 --section activity` — what already happened,
  including approaches already rejected.
- **Retry-safe:** re-running a mutation whose state already holds is a no-op
  that says so (`"noop": true` in JSON) and writes nothing. Retry freely
  after timeouts; only genuinely wrong targets fail.

## Command map

`--db PATH` is global. Filter shorts everywhere: `-p` project, `-s` status,
`-m` milestone.

| Command | Purpose |
|---|---|
| `project add\|ls\|show\|edit\|search\|archive\|unarchive` | projects + project memory |
| `issue add\|ls\|show\|edit\|mv` | the core loop |
| `issue cat` | raw stored description, never formatted |
| `issue cp` | copy an issue's shape (plan reset) — never its history |
| `issue log\|tick\|promote` | plan + activity mechanics |
| `issue append-section` | atomic append to the end of one H2 section |
| `issue lint` | validate description structure before `tick` bites |
| `issue import` | bulk create from NDJSON |
| `issue blocked\|ready` | what's stuck / what's takeable right now |
| `issue claim\|release` | session ownership on a shared board |
| `issue current` | the issue for the current git branch |
| `issue archive\|unarchive\|archive-done` | keep the board clean |
| `activity` | what changed since \<time\>, newest first |
| `project note add` | append one `###` lesson under project `## Notes` |
| `milestone add\|ls\|show\|edit\|waves` | milestones (`waves` = dependency order) |
| `label add\|ls\|rm` | labels |
| `import\|push\|sync linear` | Linear bridge — see references/linear-bridge.md |
| `tui` | interactive board (needs a TTY — not for agents) |

Bare `cliban ls|mv|rm|show|log|tick|cat` are hidden synonyms of
`issue <verb>`; GitHub reflexes `issue close|reopen|comment|delete` exist too
(`delete`/`rm` archive — nothing ever deletes). Aliases name their canonical
form and add `"canonical"` to the JSON echo; ignore unknown JSON fields.
Prefer canonical forms in anything you write down.

## Vocabulary

- Priorities: `none|low|medium|high|urgent`. Milestone statuses:
  `open|completed|cancelled` (cancelled = archived; no separate flag).
- Keys are `PROJ-42`; project keys uppercase, 2–10 chars. Sub-issue depth
  max 2. Relations: `blocks`, `blocked_by`, `related_to`.
- **Milestones are addressed by name.** Only `milestone show` takes the name
  positionally; `add`/`edit`/`rm` require `--name`.

## JSON shapes

**List rows are lean; single-entity output is complete.**

- **Lean** (`issue ls`/`blocked`/`ready`, `project ls`, `milestone ls`,
  issues under `milestone show --with-issues`): a field that is null, empty,
  or the default is **absent** — no `"milestone":null`, no `"labels":[]`, no
  `"archived":false`. Also never in list rows: `description`,
  `git_branch_name`, `position`, `created_at`. Second-precision timestamps.
  Read with `.get()` / jq; never destructure by fixed keys.
- **Full** (`issue show`, `issue current`, every mutation echo, `--full` on
  any list): all fields, optional ones `null`, microsecond timestamps. Fields:
  key, title, description, status, priority, position, archived, milestone,
  parent, due_date, labels, relations `[{type,target}]`, git_branch_name,
  created_at, updated_at, plus completed_at/claimed_by when set.

The bodies are the payload (an unscoped ls with descriptions ran to
megabytes): use `ls` to find keys, `show` to read one entity, `--full` only
deliberately and never without `-p`.

## Reads

```bash
cliban activity --since 1d -p PROJ         # merged event feed: created/completed/updated/status/edit/plan/archive/log
cliban issue ls -p PROJ -s in-progress
cliban issue ls -p PROJ -m "v0.1" --sort priority     # sort: priority|created|updated|position[:asc|desc]
cliban issue ls -p PROJ --search "ordering" --limit 20  # fuzzy; adds score; default limit 50
cliban issue ls -p PROJ --label bug --no-subs --parent PROJ-12 --updated-since 2d
cliban issue ready -p PROJ                 # takeable: backlog, unblocked, unclaimed
cliban issue blocked -p PROJ
cliban issue show PROJ-42
cliban issue show PROJ-42 --section plan   # spec|plan|activity|notes or any verbatim H2; raw md
cliban issue cat PROJ-42                   # whole description, verbatim bytes
cliban issue current                       # branch-matched issue; exit 1 = none (an answer, not a failure)
cliban milestone ls                        # unscoped = per-project counts only ({milestones,open,project})
cliban milestone ls -p PROJ --stats        # rows + done_count/last_activity; detail flags need -p
cliban milestone show "v0.1" -p PROJ --with-issues
cliban milestone waves -p PROJ "v0.1"      # {"waves":[[...],[...]],"done":[...],"external_blocked":[...]}
```

Scope every list with `-p`; unscoped `issue ls` walks every project on the
machine. `--archived` includes archived issues (excluded by default).
`activity` fields: `ts,key,project,kind,title` + `message/actor/milestone`
when set; state events carry no message, recorded ones put the detail there.
Wave N of `waves` is safe to dispatch once waves 1..N-1 are done;
`external_blocked` is gated by open work outside the milestone.

## Writes

```bash
cliban issue add -p PROJ --title "..." --description "..." --priority high \
  --label bug --label ui --milestone "v0.1" --due 2026-06-01 \
  --parent PROJ-12 --blocked-by PROJ-3 --related-to PROJ-7    # all optional but title; --status too
cliban issue edit PROJ-12 --priority urgent --label perf --remove-label stale
cliban issue edit PROJ-12 --milestone "v0.1"      # or --clear-milestone / --clear-parent
cliban issue edit PROJ-12 --blocks PROJ-9         # or --blocked-by / --related-to / --remove-relation
cliban issue cp PROJ-12 --title "Q3 edition"      # copies title/Spec/Plan(reset)/Notes/labels/priority; never history
cliban issue import ./items.ndjson                # lines: {project,title,[description,status,priority,milestone,parent,labels]}; '-' = stdin; --project fills project
cliban label add bug -p PROJ
cliban milestone add -p PROJ --name "v0.1" --target 2026-06-01
cliban milestone edit -p PROJ --name v0.1 --status completed   # or --rename / --target / --clear-target
cliban project add PROJ --name "Cliban" --description "..."
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
  re-read and retry). `project edit` takes the same flag. CAS timestamps come
  from `show`, never from list rows.

## Project memory

Durable lessons live under `## Notes` on the *project*, one `###` per lesson
— not in tickets, not in placeholder issues.

```bash
cliban project search PROJ "sqlite wal" --limit 5   # NDJSON {project,heading,content,score}; search before loading
cliban project show PROJ --section notes
cliban project note add PROJ --title "cargo test needs --test-threads=1" --body - <<'EOF'
Fixtures share a tempdir; parallel runs corrupt it.
EOF
```

## Archiving — there is no delete

```bash
cliban issue archive PROJ-12              # reversible: unarchive; key/history kept
cliban issue archive-done -p PROJ         # sweep the done column; --auto honors project policy
cliban project edit PROJ --auto-archive-done-after 7d   # 0 disables
cliban project archive KEY
cliban milestone edit -p PROJ --name v0.1 --status cancelled
```

`issue rm` / `project rm` archive and name the undo; `milestone rm` cancels.
Only `label rm` truly deletes (labels have no history).

## Exit codes

`0` ok · `1` not found (also: no section, no branch-matched issue) ·
`2` validation (bad status, bad flag, broken plan structure, stale CAS) ·
`3` internal/db error.

DB: `$XDG_DATA_HOME/cliban/cliban.db` (fallback `~/.local/share/...`);
override `--db` / `$CLIBAN_DB`.

## Don'ts

- Don't parse table output — piped output is already JSON.
- Don't pass `--editor`/`-e` without a TTY (exit 2).
- Don't note progress by rewriting descriptions (trap 1); don't log narration.
- Don't create placeholder issues as memory — `project note add` + `project search`.
- Don't start shared work unclaimed; don't finish without moving the ticket.
- Don't invent flags (no `--key`, no `issue move`, no `--all`) — on rejection
  run `cliban <cmd> --help` once instead of guessing again.
