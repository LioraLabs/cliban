# Issue tracker: cliban

Work for this repo is tracked on the local cliban board, driven by the `cliban` CLI.
Every read takes `--json`. The command surface is specified by the `cliban` skill;
the workflow contract that governs where each artifact lands is the
`cliban-workflow` skill, which ships in the separate `cliban-flow` plugin.

- **Project key:** CLI
- **Key policy:** everywhere (keys never appear in source code, prose comments, or docs as decoration; the one exception is a test citing the ticket it discharges)
- **Branch convention:** worktree-per-issue at `.worktrees/`

## Where artifacts live

| Artifact | Home |
|---|---|
| Spec / PRD | issue `## Spec` |
| Implementation plan | issue `## Plan` — parseable; mutate only via `tick`/`log`/`promote` |
| Progress, findings, dead ends | issue `## Activity Log` via `cliban issue log` |
| Durable reusable lessons | project `## Notes` — search first: `cliban project search CLI "<terms>" --json` |
| ADRs, CONTEXT.md, domain docs | this repo, plaintext, git-tracked — never cliban |

Implementation plans are deliberately not git-tracked. ADRs deliberately are.

## When a skill says "publish to the issue tracker"

```bash
cliban issue add "<title>" --project CLI --label <bug|feature|refactor|chore> \
  --description-file - --json <<'EOF'
## Spec

<the spec>
EOF
```

## When a skill says "fetch the relevant ticket"

```bash
cliban issue show KEY --json                    # whole issue
cliban issue cat KEY --section spec|plan|activity|notes
cliban issue current --json                     # issue for the current branch
```

## Blocking edges

`--blocks` / `--blocked-by` on `issue add` / `issue edit`. Relations live on the
board — never as `Blocked by:` text lines in repo files.

## Stages

The `cliban-flow` plugin carries the workflow:

- `explore-feature` — turn a rough idea into an approved design, landed as a
  ticket or an empty milestone carrying `## Spec`.
- `scope-milestone` — grill that container until the slicing is settled, then
  fill it with tracer-bullet tickets and `--blocked-by` edges.
- `triage-bug` — turn a bug report into a ticket with a reproduction, label and
  priority, after searching the board for duplicates.
- `diagnose-issue` — find and prove a root cause, logging the hypothesis ledger
  to the ticket's `## Activity Log` as it goes.
- `complete-issue` — take one ticket: `## Plan`, then test-first execution with
  `cliban issue tick` / `cliban issue log`.
- `complete-milestone` — orchestrate a whole milestone in dependency waves.

None of it is required. Plan mode or plain conversation works the same way, as
long as the artifacts land where the table above says.
