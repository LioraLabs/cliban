---
name: complete-milestone
description: "Orchestrate a cliban milestone end-to-end: one agent per ticket, dependency waves, isolated worktrees, integrated onto a milestone branch. Use when asked to run, drive, orchestrate, or complete a whole milestone."
requires_skills: [cliban-workflow]
---

# Complete Milestone

The orchestrator is a conductor, not a coder: compute the dependency order,
dispatch one agent per ticket, integrate finished work onto a milestone branch
— never `main` — until the user finalizes. Each ticket runs **`complete-issue`
in dispatched mode**, which owns its plan, implementation, verification, and
commits; this skill schedules and integrates.

**Load first:** `cliban-flow:cliban-workflow` and `cliban:cliban` — neither
loads on its own. **Not this skill:** one issue (`complete-issue`); work not
yet sliced into tickets (`scope-milestone`).

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
after each integration rather than tracking readiness yourself. A cycle exits
2 naming the issues — fix the board first. Non-empty `external_blocked` is a
stop-and-ask: work outside the milestone gates those issues.

`chains` never schedule; they staff. A chain — an author-approved `related_to`
group, a linear run of blocking edges, or same-wave tickets
predicted to touch one file — gets one implementer walking it in the
order printed, each ticket starting at its wave time from the current
milestone tip.
Reuse the implementer's context, never ticket worktrees or branches.
Split a chain only when its carrier grows too expensive (see the sweep) or the
tickets prove unrelated.

`collisions` names each path that several same-wave tickets predict in
`## Files`; those tickets are already chained, which serializes them. A
collision spanning most of a wave is a slicing problem — take it back to
`scope-milestone`. For tickets carrying no `## Files`, cross-check by hand:
the Spec's surface plus `git log --format= --name-only -- <paths>` for what
changes together; name surviving overlap in each brief. A shared surface no
test can observe always serializes, with review priced as its only gate. When
an executor finds its prediction wrong it amends `## Files`; relay that to
running siblings like any invariant.

Announce the plan: `Waves: [PROJ-5] -> [PROJ-6, PROJ-8] -> [PROJ-7]`.

## 2. Start the milestone

```bash
cliban-flow milestone start "<milestone name>" --project <KEY>
```

This creates the milestone branch in its own worktree — never the primary
checkout, which services may execute from. Root all milestone work, including
ticket worktrees, there.

## 3. Per ready ticket — worktree + one agent

Start each ticket at wave time, off the current milestone tip, never all up
front:

```bash
cliban-flow ticket start <KEY>
```

Dispatch one **`general-purpose`** agent in the printed worktree (it must
spawn its own implementer and reviewer subagents), parallel within a wave, on
the mid-tier coding model where the harness supports a per-dispatch override
(Claude Code: `model: sonnet`); reviewers stay on the session model. Task
name: lowercase the key, `-` → `_` (`CLI-95` becomes `cli_95`). Keep the
returned agent ID — liveness checks address it, not an agent type.

The brief: (1) cd into the worktree, confirm isolation, run `complete-issue`
in dispatched mode for `<KEY>`; (2) commit on the ticket branch and follow
dispatched completion through sync and ready — never integrate, move the issue
to done, or touch `main` or the milestone branch; (3) report only after ready
succeeds, with its immutable SHA, branch, test status, one-line summary, any
`## Spec` amendment, and merge risks. Never pre-plan the ticket for its agent;
do add wave-level pointers it cannot see — what siblings touch, traps already
in the INTEGRATED ledger.

**The review gate.** Before ready the implementer reports confidence and a
skip/run recommendation. Either side may request review; the orchestrator
makes the final decision at every confidence level. On skip, record
`review waived by orchestrator: <reason>` on the ticket yourself — ready
refuses a waiver logged as `agent:<KEY>`. On run, dispatch with
[review.md](../complete-issue/references/review.md) — never an improvised
brief — and have the verdict logged on the ticket
(`pass-2 review verdict: SPEC: ACCEPT, QUALITY: pass` is the canonical shape
the ready gate greps for). After a rejected review, never resume the author —
the findings brief a fresh implementer. One exception: a fix the orchestrator
can name from the finding alone it may apply itself in the ticket worktree,
logged and gated by the same fresh re-review; the first bounce goes to a fresh
agent. Two bounced re-reviews on one ticket end the loop: decide yourself —
accept with the residual risk logged, or take the slice back to
`scope-milestone` — rather than dispatch round three.

## 4. Sweep running work

An orchestrator only acts when awake, so tie the sweep to the moments it is:
at every wake — an agent report, a relayed message, a user turn — run a cheap,
read-only liveness sweep over running and dead agents:

```bash
cliban issue cat <KEY> --section plan
cliban issue cat <KEY> --section activity
git log <base>..<ticket-branch>
git -C <ticket-worktree> status -s
```

Healthy: a plan on the board and, over time, advancing activity or commits.
Do not interrupt a working agent. If all signals are empty or stale, ask the
agent — `send_message` to the agent ID saved at dispatch — for its current
phase and blocker before concluding it is stuck; a hard ticket can stay silent
a long stretch.

Cost is a sweep signal too, and yours to measure: the number that predicts the
next turn's price is current context, read from the dispatch's transcript path
without loading the transcript itself:

```bash
grep -o '"cache_read_input_tokens":[0-9]*' <agent-output-file> | tail -1
```

Past ~200k on a solo ticket or ~350k on a chain carrier, order it to commit,
write the handoff (shape: `complete-issue`'s Work step), and exit; resume on a
fresh agent from that handoff.

If the agent ID is unreachable, apply `complete-issue`'s **Resume exception**
before declaring the claimant gone, then `recover-milestone`'s ticket
interpretations: respawn work-bearing tickets in their worktree; for an empty
ticket run `cliban issue release <KEY>` and redispatch through
`ticket start <KEY>`; for an external blocker run
`cliban issue mv <KEY> blocked --note "<why>"` and surface it.
Independent siblings continue while dependents wait. After a second death on the same
ticket, stop retrying and ask the user.

## 5. Integrate as each agent finishes

A "done" notification is a claim to verify: confirm the issue is `in-review`
and the report carries the SHA returned by `ticket ready`, then:

```bash
cliban-flow ticket integrate <KEY> --dry-run
cliban-flow ticket integrate <KEY> --invariants "<what siblings could break without a conflict marker>"
```

`--invariants` is the integration relay, and yours to write — only you have
seen both sides: signature shapes, load-bearing attributes, guard ordering.
Every later `ticket sync` replays the ledger to its agent.
Stranded reviews are expected when direct delivery fails; relay the full
review to the ticket agent at integration — the ticket's logged verdict stays the durable record.

The dispatcher accepts only strict ancestry: the tested tree already contains
the exact milestone tip, so integration is a squash with no new combination of
trees and no post-integration build is needed — don't relax that guard without
restoring one. If the reported ready SHA differs from the SHA the dispatcher
recorded, a late commit exists: sync, verify, ready, and integrate again,
never copy by hand.

Scan each incoming diff for the conflicts git doesn't mark: incompatible shape
decisions on a shared function (you decide the shape and relay it), the same
helper invented twice, merge-order-sensitive shared sequences (renumber via a
dispatched ticket; no shared version bumps until finalize), a paired change
half-done (path-based hooks pass on half), and sibling overwrites of shared
board state.

## 6. Accept, sweep, finalize

The fresh assembled milestone review remains mandatory regardless of
per-ticket waivers. When every issue is `done` and the branch green: fold
milestone-relevant `## Spec` amendments into the milestone description, then
dispatch one fresh-context reviewer over the assembled milestone branch with
the milestone description (including out-of-scope decisions), the ticket
specs, and the base and tip SHAs. It verifies the promised behavior in code
and tests, records its verdict first with `cliban milestone log`, and returns
`ACCEPT` or `REJECT` with terse findings.
Do not offer finalize until the reviewer passes.

Sweep before the knowledge evaporates: promote only what no single ticket
could see — cross-ticket conflicts, hazards that fired at integration,
invariants you enforced by hand — search-first into project notes. A milestone
that taught nothing durable sweeps to zero.

Then stop and hand off: finish / PR / discard against `main` is the user's
call, especially when a phase deletes or replaces existing code.

```bash
cliban-flow milestone finish "<milestone name>" --project <KEY>    # with approval
cliban-flow milestone abandon "<milestone name>" --project <KEY> --confirm "<why>"
```
