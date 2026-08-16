---
name: recover-milestone
description: "Diagnose an interrupted cliban milestone from its durable board and git state, then recommend the existing action that resumes it."
requires_skills: [cliban-workflow]
---

# Recover Milestone

Recovery is diagnosis, not repair. Load `cliban-flow:cliban-workflow` and
`cliban:cliban`, resolve the dispatcher as the contract specifies, then survey
without changes:

```bash
cliban-flow milestone status "<milestone name>" --project <KEY>
```

Never run builds or tests during recovery — verification belongs to a
respawned ticket agent; if deliberately taken on, one worktree at a time.

## Interpret each ticket

- **Nearly finished** — commits ahead, clean worktree, no merge, completed
  plan steps: respawn in the existing worktree; it verifies, then
  `ticket status <KEY>`, `ticket sync <KEY>`, `ticket ready <KEY>`.
- **Abandoned** — no ticket commits, no dirty or merge state: after a human
  confirms discard, `ticket abandon <KEY> --confirm "<why>"`; restart later
  through `ticket start <KEY>`.
- **Silent agent** — in-progress, commits ahead, no ticked plan steps is not
  abandonment. Derive the task name from the key (`CLI-95` becomes `cli_95`),
  find it with `list_agents`, then `send_message` to its resolved agent ID. If
  absent or unreachable, load and read `complete-issue` and apply its
  **Resume exception** before deciding the claimant is gone and respawning
  onto the existing worktree to inspect and verify the work.
- **Interrupted merge** — unmerged paths belong to the implementer's ticket
  worktree, not the orchestrator: respawn there to resolve the conflicts,
  inspect the resolution diff, verify, commit, then `ticket sync <KEY>` and
  `ticket ready <KEY>`.
- **Dirty worktree** — uncommitted files are unique work until an agent has
  inspected them; never discard on age or board status alone.
- **Already integrated** — trust the recorded squash and ancestry evidence; do
  not dispatch again. A late ticket commit needs a new sync, verification, and
  ready cycle before further integration.

Compare the survey's main-drift and ancestry fields before recommending a
dispatcher action. A `[cliban-flow]` sync requested without its completion is
an interrupted operation, not proof the resulting tree was verified.

## Boundary

Do not execute repairs: no aborting merges, releasing claims, pruning
worktrees, integrating tickets, or mutating refs. Report the evidence and
recommend the applicable dispatcher or board command — the human or resumed
orchestrator judges the context the lost session may have taken with it.
