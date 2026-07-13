---
name: cliban
description: Drive the local cliban kanban board via its CLI. Use when the user mentions cliban, kanban, ticket, issue, project, milestone, or asks you to capture/move work items.
---

# Using cliban

`cliban` is a self-hosted, terminal-first kanban board with a flat CLI.
**Always use `--json` for reads** — never parse the human table format. The
default no longer opens an editor; mutations are safe to run unattended.

## Vocabulary

- **Statuses**: `backlog` | `in-progress` | `blocked` | `in-review` | `done`
- **Priorities**: `none` | `low` | `medium` | `high` | `urgent`
- **Issue keys**: `{PROJECT}-{N}` like `CLI-42` (project key is uppercase letters/digits, 2-10 chars starting with a letter).
- **Project memory**: durable context lives under `## Notes` in the project description, one retrievable lesson per `###` subsection.
- **Sub-issues**: depth limited to 2 — a sub-issue cannot have its own children. The CLI returns exit code 2 if you try to nest a third level.
- **Relations**: `blocks`, `blocked_by` (reverse of `blocks`), `related_to` (symmetric).
- **Labels**: free-form tags per project. Create with `cliban label add`, attach via `--label` on `issue add`/`edit`/`import`.

## JSON shapes

The agent-facing JSON shape is stable. Optional refs are `null` (never omitted) so destructuring is safe:

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

`cliban issue show KEY --json` returns one pretty-printed object.
`cliban issue ls --json` emits one **compact** JSON object per line (NDJSON).
Parse with `for line in stdout.splitlines(): json.loads(line)` (or `jq -c`).
`cliban project search KEY QUERY --json` emits compact NDJSON objects containing
`project`, `heading`, `content`, and `score`.

## DB location

`$XDG_DATA_HOME/cliban/cliban.db` by default (falls back to `~/.local/share/cliban/cliban.db`). Override with `--db <path>` or `$CLIBAN_DB`.

## Common recipes

### Create a project
```bash
cliban project add CLI --name "Cliban" --description "kanban board"
```

### List projects (NDJSON)
```bash
cliban project ls --json
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

### Bulk-import issues from NDJSON
```bash
cat <<'EOF' > /tmp/imp.ndjson
{"project":"CLI","title":"alpha","priority":"high","labels":["bug"]}
{"project":"CLI","title":"beta","milestone":"v0.1","blocked_by":"CLI-1"}
EOF
cliban issue import /tmp/imp.ndjson --json
# or stream:
cliban issue import - < /tmp/imp.ndjson --json
```
Each input line is a `{project, title, [description, status, priority, milestone, parent, labels]}` object. With `--project KEY`, records may omit `project`.

### Add a sub-issue
```bash
cliban issue add --project CLI --parent CLI-12 \
  --title "Repro test" --priority medium --json
```

### Read multi-line description from a file
```bash
cliban issue add --project CLI --title "Plan" --description-file ./plan.md
# stdin still works:
cliban issue edit CLI-12 --description - < /tmp/desc.md
```

### Store and retrieve persistent agent memory
```bash
cliban project add CLI --name "Cliban" --description-file project.md
cliban project search CLI "sqlte canonical" --section notes --json
cliban project show CLI --section notes
cliban project edit CLI --description-file - < updated-project.md
```
Use the project description's `## Notes` section for durable knowledge that
does not belong in an issue. Give each lesson a descriptive `###` heading.
Search first: every whitespace-separated term is fuzzy-matched against each
heading and body, and only matching subsections are returned. Results are
ranked and capped by `--limit` (default 20), so do not load the complete notes
section unless the task needs it. `--section all` searches all `###`
subsections. Project `add` and `edit` accept `--description-file`; `-` reads
stdin.

### Move work along
```bash
cliban issue mv CLI-12 in-progress
cliban issue mv CLI-12 in-review
cliban issue mv CLI-12 done
```

### Set or clear a milestone
```bash
cliban milestone add --project CLI --name "v0.1" --target 2026-06-01
cliban milestone show v0.1 --project CLI --with-issues --json   # positional NAME
cliban issue edit CLI-12 --milestone "v0.1"
cliban issue edit CLI-12 --clear-milestone
```

### Clear a parent (promote a sub-issue back to top level)
```bash
cliban issue edit CLI-12 --clear-parent
```

### Labels
```bash
cliban label add bug --project CLI
cliban label ls --project CLI --json
cliban issue edit CLI-12 --label bug --label cook-cc
cliban issue ls --project CLI --label bug --json   # filter (all-of semantics)
cliban issue edit CLI-12 --remove-label cook-cc
```

### Issue relations
```bash
cliban issue edit CLI-12 --blocks CLI-9
cliban issue edit CLI-12 --blocked-by CLI-3
cliban issue edit CLI-12 --related-to CLI-7

cliban issue blocked --project CLI --json    # issues with an open blocker
cliban issue edit CLI-12 --remove-relation CLI-9
```

### Sorting
```bash
cliban issue ls --project CLI --sort priority --json          # urgent first (default desc)
cliban issue ls --project CLI --sort created:asc --json
cliban issue ls --project CLI --sort updated:desc --json
cliban issue ls --project CLI --sort position --json
```

### Inspect a single issue (full detail)
```bash
cliban issue show CLI-42 --json
```

### Delete (cascades sub-issues)
```bash
cliban issue rm CLI-12
```

### Archive a single issue (hide from default board/list)
```bash
cliban issue archive CLI-12
cliban issue unarchive CLI-12        # restore
```

### Sweep done issues out of a project board
```bash
cliban issue archive-done --project CLI --json
# Or run the auto sweep that honors each project's policy:
cliban project edit CLI --auto-archive-done-after 7d
cliban issue archive-done --auto --json
```
Setting `--auto-archive-done-after 0` disables the policy.

### Query archived issues
```bash
cliban issue ls --project CLI --archived --json
```

## Exit codes

- `0` success
- `1` not found
- `2` validation error (invalid status, depth-2 violation, missing required flag, etc.)
- `3` internal/db error

## What NOT to do

- Don't try to parse the table output of `ls`/`show`. Use `--json`.
- Don't nest sub-issues three levels deep; the CLI returns exit code 2.
- Don't filter on archived state by hand — pass `--archived` to `ls` to include them; otherwise they are excluded.
- Don't assume timestamps are in the local timezone — they are UTC ISO-8601.
- Don't create fake backlog/done issues for agent memory. Use `###` subsections under the project's `## Notes` and retrieve them with `project search`.
- Don't pass `--editor` in an agent context unless you actually have a TTY; it will fail with exit code 2 if stdin isn't a TTY.

## Editor behavior (agent-safe)

`cliban issue add` and `cliban issue edit` **never open an editor by default**.
You must pass `--editor` (or `-e` for `edit`) to opt in. Without `--editor`,
`add` requires `--title`; `edit` requires at least one mutation flag — both
fail with exit code 2 otherwise. The legacy `--no-editor` flag is still
accepted as a no-op for backwards compatibility.

## Discovery checklist

When the user gives a vague kanban-related task, run these reads first to ground yourself in the current state:

```bash
cliban project ls --json
cliban issue ls --status in-progress --json
cliban issue ls --status blocked --json
cliban issue blocked --json            # what's stuck on something
```
