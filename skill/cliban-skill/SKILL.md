---
name: cliban
description: Drive the local cliban kanban board via its CLI. Use when the user mentions cliban, kanban, ticket, issue, project, milestone, or asks you to capture/move work items.
---

# Using cliban

`cliban` is a self-hosted, terminal-first kanban board with a flat CLI. **Always use `--json` for reads** — never parse the human table format. **Always pass content flags on mutations** — never let the editor open (or pass `--no-editor`) so the agent never hangs on an interactive editor.

## Vocabulary

- **Statuses**: `backlog` | `in-progress` | `blocked` | `in-review` | `done`
- **Priorities**: `none` | `low` | `medium` | `high` | `urgent`
- **Issue keys**: `{PROJECT}-{N}` like `CLI-42` (project key is uppercase letters/digits, 2-10 chars starting with a letter).
- **Sub-issues**: depth limited to 2 — a sub-issue cannot have its own children. The CLI returns exit code 2 if you try to nest a third level.

## DB location

`$XDG_DATA_HOME/cliban/cliban.db` by default (falls back to `~/.local/share/cliban/cliban.db`). Override with `--db <path>` or `$CLIBAN_DB`.

## Common recipes

### Create a project
```bash
cliban project add CLI --name "Cliban" --description "kanban board"
```

### List projects (machine-readable)
```bash
cliban project ls --json
```

### Capture a new issue
```bash
cliban issue add --project CLI \
  --title "Fix the kanban column ordering" \
  --description "When more than 5 cards exist in IN-REVIEW, positions go negative." \
  --priority high --json
```

### Add a sub-issue
```bash
cliban issue add --project CLI --parent CLI-12 \
  --title "Repro test" --priority medium --json
```

### List blocked issues across all projects
```bash
cliban issue ls --status blocked --json
```

### Move work along
```bash
cliban issue mv CLI-12 in-progress
cliban issue mv CLI-12 in-review
cliban issue mv CLI-12 done
```

### Set or clear a milestone
```bash
cliban milestone add --project CLI --name "v0.1" --target 2026-06-01
cliban issue edit CLI-12 --milestone "v0.1"
cliban issue edit CLI-12 --clear-milestone
```

### Clear a parent (promote a sub-issue back to top level)
```bash
cliban issue edit CLI-12 --clear-parent
```

### Pipe a multi-line description from a file
```bash
cliban issue edit CLI-12 --description - < /tmp/desc.md
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

### Sweep all done issues out of a project board
```bash
cliban issue archive-done --project CLI --json
```

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

- Don't invoke `cliban issue add --project X` with no `--title` — that triggers the editor and will hang in agent contexts. Always pass `--title` (and `--description`/`--priority` as needed).
- Don't try to parse the table output of `ls`/`show`. Use `--json` (NDJSON for lists, single JSON object for `show`).
- Don't nest sub-issues three levels deep; the CLI returns exit code 2.
- Don't filter on archived state by hand — pass `--archived` to `ls` to include them; otherwise they are excluded.
- Don't assume timestamps are in the local timezone — they are UTC ISO-8601.
- Don't try to mutate via `cliban issue edit CLI-42` with no flags. That triggers the editor. Pass `--title`/`--description`/etc. or use `--no-editor` to fail fast.

## Discovery checklist

When the user gives a vague kanban-related task, run these reads first to ground yourself in the current state:

```bash
cliban project ls --json
cliban issue ls --status in-progress --json
cliban issue ls --status blocked --json
```
