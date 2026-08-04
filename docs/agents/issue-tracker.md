# Issue tracker: cliban

Work for this repo is tracked on the local cliban board, driven by the `cliban` CLI.
Every read takes `--json`; the description contract and mutation commands are
specified by the cliban plugin's `cliban-workflow` skill.

- **Project key:** CLI
- **Craft stack:** mattpocock-skills
- **Key policy:** everywhere (keys never appear in source code, comments, or docs under any policy)
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

## Stage mapping

The mattpocock-skills stack owns the craft; this contract owns where the
artifacts live. Reach a design however you like (grilling, plan mode, plain
conversation), then:

- `/to-spec` synthesizes the conversation into a spec and publishes it to the
  issue's `## Spec` section.
- `/to-tickets` breaks a spec or plan into tracer-bullet tickets as cliban
  issues, declaring blocking edges natively with `--blocks` / `--blocked-by`.
- `/implement` fetches the ticket from cliban, writes `## Plan`, and drives TDD
  (red → green) with `cliban issue tick` / `cliban issue log`.
- `/triage` labels are ordinary cliban labels.
- `/wayfinder` maps huge efforts as a `wayfinder:map` issue whose tickets are
  child issues on this board.

This file doubles as the `docs/agents/issue-tracker.md` that
`setup-matt-pocock-skills` would write for an "other" tracker — do not run
that setup's tracker section on top of it.
