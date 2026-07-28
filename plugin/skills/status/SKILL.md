---
name: status
description: "Project progress tracking via cliban. Use when user invokes /status to see active projects or project details."
requires_skills: [cliban-workflow]
---

# Status — Project Progress

Show cliban project progress. Replaces the older memory-API-backed implementation.

## Subcommands

### `/status` (no args)

List all active projects with progress counts:

```bash
cliban project ls --json
```

For each project, also fetch issue counts per status:

```bash
for status in backlog in-progress blocked in-review done; do
  count=$(cliban issue ls --project <KEY> --status "$status" --json | wc -l)
  echo "  $status: $count"
done
```

Aggregate into one summary per project. Skip the `done` count if the project has auto-archive enabled (it'll always be small).

### `/status <project>`

Show details for a specific project (case-insensitive match against `key` or `name`):

```bash
# Resolve project key
TARGET="<PROJECT_INPUT>"
KEY=$(cliban project ls --json | jq -r --arg t "$TARGET" \
  'select((.key|ascii_upcase) == ($t|ascii_upcase) or (.name|ascii_upcase) == ($t|ascii_upcase)) | .key' | head -1)

# Fetch project + active issues
cliban project ls --json | jq --arg k "$KEY" 'select(.key == $k)'
cliban issue ls --project "$KEY" --status in-progress --json
cliban issue ls --project "$KEY" --status blocked --json
cliban milestone ls --project "$KEY" --json
```

### `/status blocked`

Cross-project view of issues with open blockers:

```bash
cliban issue blocked --json
```

## Output Format

For project list (`/status`):

```
<KEY>  <Name>
  backlog:     <n>
  in-progress: <n>
  blocked:     <n>
  in-review:   <n>
  (done counts skipped when auto-archive enabled)
```

For a single project (`/status <project>`):

```
<KEY> — <Name>
<description first line, if present>

Active milestones:
  <name>  target: YYYY-MM-DD  issues: <n>

In progress:
  <KEY-N>  <title>  (priority)
  ...

Blocked:
  <KEY-N>  <title>  blocked by: <KEY-M>
  ...
```

For `/status blocked`:

```
Blocked issues (across all projects):
  <KEY-N>  <title>  blocked by: <KEY-M>  (priority)
  ...
```

## Notes

- All reads use `--json`. Never parse the table output.
- Done issues are excluded by default (use `--archived` to include archived items if specifically asked).
- The previous version stored project memories in an external API; that backing store is no longer queried. Project state lives in cliban.
