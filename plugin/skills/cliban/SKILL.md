---
name: cliban
description: Drive the local cliban kanban board via its CLI. Use when the user mentions cliban, kanban, ticket, issue, project, milestone, or asks you to capture/move work items, or asks what changed/what happened on the board.
---

# Using cliban

A terminal-first kanban board built for agents. One grammar covers the whole
surface; anything not written here follows from it, from `--help`, and from
error messages that name the fix.

    cliban <noun> <verb> [IDENTITY] [flags]

- **Nouns**: `project`, `issue`, `milestone`, `label`, `linear`, `activity`
  (the event feed), `tui` (humans only).
- **Identity is positional**: `issue show PROJ-42`, `issue add "Fix
  ordering"`, `milestone edit "v0.1" --status completed`. Milestones go by
  name (project-scoped), issues by key. A flag-looking value needs the
  standard `--` escape. Required flags don't exist: what a command can infer
  (single-task `--task`, a promoted step's title, a project's display name)
  it infers.
- **Filters are short flags**: `-p` project KEY, `-s` status, `-m` milestone.
- **Ambient scope**: `$CLIBAN_PROJECT` (per-repo via direnv) is the default
  `-p` everywhere; explicit `-p KEY` beats it, `-p '*'` widens to every
  project. It also stands in for the positional KEY on project reads and
  memory appends (`project show|cat|search|note add`) — never on structural
  writes (`edit`, `archive`).
- **Three viewers, one job each**: `ls` = many lean rows; `show` = one
  complete entity; `cat` = raw markdown bytes (whole description or
  `--section X`), never formatted.
- **Output follows the reader**: piped stdout → JSON/NDJSON, TTY → tables;
  `--json`/`--table` force it, `$CLIBAN_OUTPUT` pins it. Mutations echo one
  compact lean JSON line when piped. `cat` is the exception: always bytes.

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
lands on the issue's timeline, attributed to `$CLIBAN_ACTOR` or the ambient
Claude session. You supply the **why**:

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
  `issue promote PROJ-42 --task 1 --step 3` (`--as sub-issue|related`), or
  file a new issue with `--blocked-by`.
- Before starting: `activity --since 3d` (the board lately) and
  `activity --issue PROJ-42` (one ticket's whole merged history, including
  approaches already tried and rejected).
- **Retry-safe:** a mutation whose state already holds is a no-op that says
  so (`"noop":true`) and writes nothing. Retry freely after timeouts.

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
| `issue current` | the issue for the current git branch (exit 1 = none) |
| `issue cp` | copy an issue's shape (plan reset) — never its history |
| `issue import` | bulk create from NDJSON: `{project,title,[description,status,priority,milestone,parent,labels]}` per line |
| `issue archive\|unarchive\|archive-done` | keep the board clean; nothing ever deletes |
| `activity` | what changed since \<time\>; `--issue KEY` = one full history |
| `milestone add\|ls\|show\|edit\|waves` | milestones; `waves` = dependency order for parallel dispatch |
| `label add\|ls\|rm` | labels (`label rm` deletes — labels have no history) |
| `linear import\|push\|sync` | Linear bridge — read references/linear-bridge.md first |

## Vocabulary

- Priorities: `none|low|medium|high|urgent`. Milestone statuses:
  `open|completed|cancelled` (cancelled = archived; no separate flag).
- Keys are `PROJ-42`; project keys uppercase, 2–10 chars. Sub-issue depth
  max 2. Relations: `blocks`, `blocked_by`, `related_to` (set via those
  flags on `issue add`/`edit`; `--remove-relation` detaches).

## JSON shapes

**List rows are lean; single-entity output is complete.**

- **Lean** (every `ls`, every mutation echo): a field that is null, empty,
  or the default is **absent**; `description`, `git_branch_name`,
  `position`, `created_at` never appear; second-precision timestamps. One
  exception: a mutation echo's `updated_at` keeps microsecond precision —
  **the echo is a valid `--if-updated-at` CAS token**, so edits chain
  without a re-`show`. Read with `.get()` / jq; never destructure by fixed
  keys.
- **Full** (`issue show`, `issue current`, `--full` on any list): all
  fields, optional ones `null`, microsecond timestamps, plus
  `completed_at`/`claimed_by` when set.

The bodies are the payload: `ls` to find keys, `show` to read one entity,
`--full` only deliberately and never without a project scope.

## The non-obvious reads

```bash
cliban issue ls --ready                    # takeable: backlog, unblocked, unclaimed — composes with every filter
cliban issue ls --blocked                  # at least one open blocker
cliban issue ls --search "ordering"        # fuzzy across title/key/labels/description; adds score
cliban issue cat PROJ-42 --section plan    # section body: spec|plan|activity|notes or any verbatim H2
cliban milestone ls                        # scoped: lean rows · unscoped: per-project counts only
cliban milestone waves "v0.1"              # {"waves":[[...],...],"done":[...],"external_blocked":[...]}
```

Wave N of `waves` is dispatchable once waves 1..N-1 are done;
`external_blocked` is gated by open work outside the milestone. `activity`
fields: `ts,key,project,kind,title` + `message/actor/milestone` when set.

## The description contract

Four H2 anchors are reserved, matched exactly: `## Spec`, `## Plan`,
`## Activity Log`, `## Notes`. Other H2s are yours; section tools address
them by verbatim anchor. Plan tasks are `### Task N:` headings; steps are
column-zero GFM checkboxes (`- [ ] **Step 1: ...**`).

```bash
cliban issue edit PROJ-12 --section spec --description-file -   # replace ONE section, others byte-identical
cliban issue append-section PROJ-12 --section notes "- appended to the END of the section"
cliban issue lint PROJ-12               # exit 2 + findings when tick would choke
cliban issue log PROJ-12 "note"         # writes ## Activity Log AND the durable record
```

- A missing section on write is exit 2 listing the ones that exist;
  `--create-section` when you mean to add one. Payloads are the section
  *body* — an inner H2 is exit 2. `--section activity` is refused on
  writes: the log belongs to `issue log`.
- `issue log` / `append-section` / `project note add` read piped stdin when
  the text argument is absent.
- Racy round-trips: pass `--if-updated-at <updated_at>` from a prior `show`
  or echo (exit 2 = stale; re-read and retry). `project edit` takes it too.

## Project memory

Durable lessons live under `## Notes` on the *project*, one `###` per lesson
— not in tickets, not in placeholder issues. Search first; don't load the
whole section.

```bash
cliban project search "sqlite wal"             # NDJSON {project,heading,content,score}
cliban project cat --section notes
cliban project note add "cargo test needs -j1" --body -   # '-' = stdin
```

## Exit codes

`0` ok · `1` not found (also: no section, no branch-matched issue) ·
`2` validation (bad status, bad flag, missing scope, broken plan structure,
stale CAS) · `3` internal/db error.

DB: `$XDG_DATA_HOME/cliban/cliban.db` (fallback `~/.local/share/...`);
override `--db` / `$CLIBAN_DB`.

## Don'ts

- Don't note progress by rewriting descriptions (trap 1); don't log narration.
- Don't create placeholder issues as memory — `project note add` + `project search`.
- Don't start shared work unclaimed; don't finish without moving the ticket.
- Don't pass `--editor` without a TTY, and don't parse table output.
- Don't invent flags or verbs — on rejection run `cliban <cmd> --help` once
  instead of guessing again.
