---
name: recover-milestone
description: "Diagnose an interrupted cliban milestone from its durable board and git state, then recommend the existing action that can resume it."
requires_skills: [cliban-workflow]
---

# Recover Milestone

Recovery is diagnosis, not repair. Load `cliban-workflow` and `cliban`, resolve
its dispatcher as the workflow contract specifies, then survey without changes:

```bash
cliban-flow milestone status "<milestone name>" --project <KEY>
```

Never run builds or tests during recovery. If verification is deliberately
taken on by the orchestrator, do it in one worktree at a time. Normal
verification belongs to a respawned ticket agent.

## Interpret each ticket

- **Nearly finished:** commits ahead, a clean worktree, no merge, and completed
  plan steps. Respawn the agent in the existing worktree; it verifies and uses
  `ticket status <KEY>`, `ticket sync <KEY>`, then `ticket ready <KEY>`.
- **Abandoned:** no ticket commits and no dirty or merge state. After a human
  confirms discard, recommend `ticket abandon <KEY> --confirm "<why>"`; restart
  later through `ticket start <KEY>`.
- **Silent agent:** `in-progress`, commits ahead, and no ticked plan steps is not
  abandonment. Derive the dispatch task name from the ticket key by lowercasing
  and replacing `-` with `_` (`CLI-95` becomes `cli_95`), find it with the agent
  runtime's `list_agents` operation, then use `send_message` with its resolved
  agent ID. If absent or unreachable, load and read `complete-issue`, then apply
  its **Resume exception** before deciding the claimant is gone and respawning
  onto the existing worktree to inspect and verify the work.
- **Interrupted merge:** unmerged paths belong to the implementer's ticket
  worktree, not the orchestrator. Respawn there to resolve the conflicts,
  inspect the resolution diff, verify, commit, and continue with `ticket sync
  <KEY>` and `ticket ready <KEY>`.
- **Dirty worktree:** treat uncommitted files as unique work until an agent has
  inspected them. Never discard them based only on age or board status.
- **Already integrated:** trust the recorded squash and ancestry evidence; do
  not dispatch the ticket again. A late ticket commit requires a new sync,
  verification, and ready cycle before any further integration.

Compare the survey's main-drift and ancestry fields before recommending a
dispatcher action. Read each ticket's last recorded `[cliban-flow]` action: a
requested sync without its completion is an interrupted operation, not proof
that the resulting tree was verified.

## Boundary

Do not execute repairs. Report the evidence and recommend the applicable
dispatcher or board command. In particular, do not abort merges, release
claims, prune worktrees, integrate tickets, or mutate refs. Those actions need
the human or resumed orchestrator to judge the context that the lost session
may have taken with it.
