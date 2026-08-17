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

A ticket's plan is the recovery map: ticked tasks are work a verifier passed,
the last logged seam verdict says how far judgment reached, and a `Ruling:` line
records a decision the lost planner already made. Read all three before
concluding anything from git alone.

## Interpret each ticket

- **Nearly finished** — commits ahead, clean worktree, no merge, every plan task
  ticked and the final seam reviewed: respawn in the existing worktree; it
  verifies, then `ticket status <KEY>`, `ticket sync <KEY>`, `ticket ready <KEY>`.
- **Mid-loop** — commits ahead of the last ticked task, or a task ticked with no
  verdict where the plan marks a seam: the loop stopped between dispatches.
  Respawn the planner in the worktree; it re-runs the verifier over the
  unaccounted range rather than trusting the commit, and picks the loop back up.
- **Abandoned** — no ticket commits, no dirty or merge state: after a human
  confirms discard, `ticket abandon <KEY> --confirm "<why>"`; restart later
  through `ticket start <KEY>`.
- **Silent agent** — in-progress, commits ahead, no ticked plan tasks is not
  abandonment: silence is indistinguishable from progress, and a false "dead"
  verdict costs more than a slow one. A delegate dies with the session that
  dispatched it, so a claimant from an interrupted run is already gone — load
  `complete-issue`, apply its **Resume exception**, and respawn a fresh
  session onto the existing worktree to inspect and verify the work before
  anything is discarded.
- **Interrupted merge** — unmerged paths belong to the ticket's own worktree,
  not the orchestrator: respawn there to resolve the conflicts, inspect the
  resolution diff, verify, commit, then `ticket sync <KEY>` and
  `ticket ready <KEY>`.
- **Dirty worktree** — uncommitted files are unique work until an agent has
  inspected them; never discard on age or board status alone.
- **Already integrated** — trust the recorded squash and ancestry evidence; do
  not dispatch again. A late ticket commit needs a new sync, verification, and
  ready cycle before further integration.

Compare the survey's main-drift and ancestry fields before recommending a
dispatcher action. A `[cliban-flow]` sync requested without its completion is
an interrupted operation, not proof the resulting tree was verified. A wave
whose tickets all integrated but whose wave review never logged a verdict is an
unreviewed wave, not a finished one.

## Boundary

Do not execute repairs: no aborting merges, releasing claims, pruning
worktrees, integrating tickets, or mutating refs. Report the evidence and
recommend the applicable dispatcher or board command — the human or resumed
orchestrator judges the context the lost session may have taken with it.
