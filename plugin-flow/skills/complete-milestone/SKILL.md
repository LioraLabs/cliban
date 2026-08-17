---
name: complete-milestone
description: "Orchestrate a cliban milestone end-to-end: one agent per ticket, dependency waves, isolated worktrees, integrated onto a milestone branch. Use when asked to run, drive, orchestrate, or complete a whole milestone."
requires_skills: [cliban-workflow]
---

# Complete Milestone

A conductor, not a coder, and not a planner either: schedule the waves, dispatch
one planner per ticket, integrate what comes back, review each assembled wave,
and hand the branch to the user. How a ticket gets done is
[complete-issue](../complete-issue/SKILL.md)'s business — you never plan, write,
or fix inside one.

**Load first:** `cliban-flow:cliban-workflow` and `cliban:cliban` — neither
loads on its own. **Not this skill:** one issue (`complete-issue`); work not yet
sliced into tickets (`scope-milestone`).

Ticket branches merge into the milestone integration branch, never `main`, so
`main` stays shippable; the user lands the branch as one atomic switch at the
end.

```
main ──────────────────────────────────●  (untouched until final cutover)
       └─ milestone/<slug> ──●──●──●────┘  (each ● = one ticket integrated)
                    .worktrees/<ticket-branch> per ticket
```

## 1. Compute the waves

```bash
cliban milestone waves --project <KEY> "<milestone name>" --json
```

`waves[0]` is dispatchable now; wave N is safe once 1..N-1 have merged. Re-run
after each integration rather than tracking readiness yourself. A cycle exits 2
naming the issues — fix the board first. Non-empty `external_blocked` is a
stop-and-ask: work outside the milestone gates those issues.

Blocking edges are the only thing that orders the work. One ticket, one planner,
started at its wave time from the current milestone tip — never carry a planner
across tickets to save it re-reading, because a planner that delegates has
little to re-read.

`collisions` names each path that several same-wave tickets predict in
`## Files`. It briefs; it does not schedule. Name the overlap in each colliding
ticket's brief and dispatch them in parallel anyway — the prediction is
file-granular, so it cannot tell two line-adds from two incompatible designs,
and serializing on it costs the whole wave. Overlap that survives is the wave
review's job. Chain a colliding pair only when the overlap is one function or
type both must reshape, or after an integration actually blew up on it; a
collision spanning most of a wave is a slicing problem — take it back to
`scope-milestone`. When a planner finds its prediction wrong it amends
`## Files`; relay that to running siblings like any invariant.

Announce the plan: `Waves: [PROJ-5] -> [PROJ-6, PROJ-8] -> [PROJ-7]`.

## 2. Start the milestone

```bash
cliban-flow milestone start "<milestone name>" --project <KEY>
```

This creates the milestone branch in its own worktree — never the primary
checkout, which services may execute from. Root all milestone work, including
ticket worktrees, there.

## 3. Dispatch one planner per ticket

Start each ticket at wave time, off the current milestone tip, never all up
front:

```bash
cliban-flow ticket start <KEY>
```

Dispatch one **`general-purpose`** planner in the printed worktree, parallel
within a wave, **on the session model** — it runs `complete-issue` in dispatched
mode and spawns its own implementers, verifiers, and reviewers, and a planner
demoted to the cheap model plans cheaply. The mid-tier demotion happens one
level below you, and is its call, not yours. Task name: lowercase the key,
`-` → `_` (`CLI-95` becomes `cli_95`). Keep the returned agent ID: it is the
address for one-way brief amendments — a sibling's invariant, an amended
`## Files` — relayed to a running planner with no reply awaited; a delegate's
answers arrive only as its completion report.

The brief: cd into the worktree, confirm isolation, run `complete-issue` for
`<KEY>`, follow dispatched completion through sync and ready, and report only
after ready succeeds — its final message carries the immutable SHA, branch,
seam verdicts, test status, a one-line summary, any `## Spec` amendment, and
merge risks. It never integrates, moves the issue to done, or touches `main`
or the milestone branch. Never pre-plan the ticket; do add the wave-level
pointers it cannot see — what siblings touch, traps already in the INTEGRATED
ledger.

## 4. On every wake

Wakes arrive on their own: each dispatched planner's completion — finished,
struck out, or dead, the harness reports all three — re-invokes you, as does a
user turn. Between wakes you are not running and nothing is lost; the board
and git hold the milestone's whole state, so ending your turn with planners
in flight is the milestone running. On each wake, before acting on the report
that caused it, derive where everything stands:

```bash
cliban issue cat <KEY> --section plan
cliban issue cat <KEY> --section activity
git log <base>..<ticket-branch>
git -C <ticket-worktree> status -s
```

The board outranks your memory, always, and after a compaction especially: waves
recomputed, ticked plans, and the `[cliban-flow]` activity lines say what has
happened. Re-dispatching a ticket you cannot remember integrating is the most
expensive mistake available to you, and the derivation is what prevents it.

Healthy: a plan with seams on the board and, over time, ticking tasks,
advancing activity, or commits. Silence is indistinguishable from progress — a
hard ticket can stay quiet a long stretch — so judge by the artifacts and let
a working delegate work. A completion that arrives without a ready SHA is a
death or a strike-out: read `recover-milestone` and follow its interpretation
of that ticket's state — it owns them. Independent siblings continue while
dependents wait. After a second death on the same ticket, stop retrying and
ask the user.

## 5. Integrate

A completion report is a claim to verify: confirm the issue is `in-review` and
the report carries the SHA returned by `ticket ready`. Then decide pass 2 per
[review.md](../complete-issue/references/review.md), which owns the review
contract — the planner is gone, and its recommendation is in the report. On
run, dispatch a fresh reviewer over the ticket diff at the ready SHA; findings
brief a fresh planner on the same ticket, which lands a new ready SHA. On
skip, record the waiver — ready refuses one written by the ticket's own agent:

```bash
cliban issue log <KEY> "review waived by orchestrator: <reason>"
```

Then:

```bash
cliban-flow ticket integrate <KEY> --dry-run
cliban-flow ticket integrate <KEY> --invariants "<what siblings could break without a conflict marker>"
```

`--invariants` is the integration relay, and yours to write — only you have seen
both sides: signature shapes, load-bearing attributes, guard ordering. Every
later `ticket sync` replays the ledger to its agent.

The dispatcher accepts only strict ancestry: the tested tree already contains
the exact milestone tip, so integration is a squash with no new combination of
trees and no post-integration build is needed — don't relax that guard without
restoring one. If the reported ready SHA differs from the SHA the dispatcher
recorded, a late commit exists: sync, verify, ready, and integrate again, never
copy by hand.

## 6. Review each assembled wave

When a wave's last ticket integrates, before the next wave branches off the new
tip, dispatch one fresh-context reviewer over the assembled wave — base = the
tip before the wave, head = the tip after it. **A wave of one skips this:** its
ticket's own seams already reviewed that diff, and there are no siblings to
collide with.

The wave review hunts what no ticket agent could see and git does not mark:
incompatible shape decisions on a shared function (you decide the shape and
relay it), the same helper invented twice, merge-order-sensitive shared
sequences (renumber via a dispatched ticket; no shared version bumps until
finalize), a paired change half-done (path-based hooks pass on half), and
sibling overwrites of shared board state. Record its verdict with
`cliban milestone log`. Findings become work: a fresh ticket, or a fresh
implementer in the milestone worktree for a fix you can name from the finding
alone — never your own edit, which skips the gate that found it.

## 7. Accept, sweep, finalize

The fresh assembled milestone review remains mandatory regardless of every
waiver below it. When every issue is `done` and the branch is green, fold
milestone-relevant `## Spec` amendments into the milestone description, then
dispatch one fresh-context reviewer over the assembled milestone branch with the
milestone description (including out-of-scope decisions), the ticket specs, the
milestone log — where the planners left their accepted Minors, rulings, and
traps — and the base and tip SHAs. It verifies the promised behavior in code and tests,
records its verdict first with `cliban milestone log`, and returns `ACCEPT` or
`REJECT` with terse findings. Do not offer finalize until the reviewer passes.

Sweep before the knowledge evaporates: promote only what no single ticket could
see — cross-ticket conflicts, hazards that fired at integration, invariants you
enforced by hand — search-first into project notes. A milestone that taught
nothing durable sweeps to zero.

Then stop and hand off: finish / PR / discard against `main` is the user's call,
especially when a phase deletes or replaces existing code.

```bash
cliban-flow milestone finish "<milestone name>" --project <KEY>    # with approval
cliban-flow milestone abandon "<milestone name>" --project <KEY> --confirm "<why>"
```
