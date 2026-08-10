---
name: complete-milestone
description: "Orchestrate an entire cliban milestone end-to-end — one agent per ticket, run in dependency waves, each isolated in its own worktree, integrated onto a milestone branch. Use when asked to orchestrate, drive, or complete a whole milestone of dependency-linked issues."
requires_skills: [cliban-workflow]
---

# Complete Milestone

The orchestrator is a conductor, not a coder: it computes the dependency order, dispatches one agent per ticket, gates each on its dependencies, and integrates finished work onto a milestone branch — never `main` — until the user finalizes.

Inside each ticket runs **`complete-issue` in dispatched mode**, which owns claiming, planning, test-first execution, and committing. This skill schedules that and integrates the results; it does not restate the rhythm.

**Load first:** invoke `cliban-flow:cliban-workflow` for the contract (status mapping, where each artifact lands) and `cliban:cliban` for CLI mechanics. Neither loads on its own — reach for them with the Skill tool before the first board action.

**Not this skill:** a single issue (`complete-issue`); work not yet sliced into tickets (`scope-milestone`); issues with no shared integration target.

## The integration branch rule

Ticket branches merge into a milestone integration branch, never `main`. Merging half-finished phases into `main` breaks whatever builds from it — often the very tool you're tracking the work with. `main` stays shippable; the milestone branch absorbs the in-progress phases; the user lands it as one atomic switch at the end.

```
main ──────────────────────────────────────●  (untouched until final cutover)
        │                                   ↑
        └─ milestone/<slug> ──●──●──●──●─────┘  (each ● = one ticket merged in)
                              │  │  │  │
                       .worktrees/<ticket-branch> per ticket
```

## 1. Compute the waves

Don't derive the partition by hand — the CLI computes it from the blocking graph:

```bash
cliban milestone waves --project <KEY> "<milestone name>" --json
# {"waves":[["PROJ-5"],["PROJ-6","PROJ-8"],["PROJ-7"]], "done":[…], "external_blocked":[…]}
```

`waves[0]` is dispatchable now; wave N is safe once 1..N-1 have merged. Re-run after each integration rather than tracking readiness yourself.

- **A cycle exits 2** naming the issues — fix the board before orchestrating.
- **Non-empty `external_blocked`** is a stop-and-ask: those issues are gated by work *outside* the milestone, and no amount of wave-finishing frees them.

Announce the plan: `Waves: [PROJ-5] -> [PROJ-6, PROJ-8] -> [PROJ-7]`.

## 2. Start the milestone

```bash
cliban-flow milestone start "<milestone name>" --project <KEY>
```

This creates the milestone branch in its own worktree, never in the primary checkout. The separation matters because services and watchers may execute directly from the primary checkout; changing its branch can live-patch them. Keep all milestone work, including ticket worktrees, rooted there.

## 3. Per ready ticket — worktree + one agent

Start each ticket **at wave time**, off the current milestone tip, never all up front — otherwise dependent tickets begin without their dependency's work.

```bash
cliban-flow ticket start <KEY>
```

The command prints the ticket worktree path. Dispatch one agent per ticket there, parallel within a wave, as **`general-purpose`** — it has to spawn its own implementer and reviewer subagents, which tool-restricted types (`Explore`, `Plan`) cannot do.

The brief:

1. `cd` into the worktree, confirm isolation, then run `complete-issue` in **dispatched mode** for `<KEY>`.
2. Commit on `<ticket-branch>`, follow dispatched completion through `ticket sync` and `ticket ready`, then report. Do not integrate, move the issue to `done`, or touch `main` or the milestone branch.
3. Report only after `ticket ready` succeeds. Include its immutable SHA, branch, test status, one-line summary, any `## Spec` amendment, and merge-risk notes. Never commit after ready; a changed branch must repeat sync, verification, and ready.

Never pre-plan a ticket for its agent. The agent runs plan and execute itself; that's where the per-ticket review checkpoints live.

## 4. Integrate as each agent finishes

The orchestrator integrates, not the agent. **A "done" notification is a claim to verify, not a fact.** Confirm the issue is `in-review`, its plan is fully ticked, and the report includes the SHA returned by `ticket ready`, then:

Stranded reviews are expected when direct agent delivery fails. Relay the full
review to the ticket agent as part of integration; the ticket's concise verdict
and findings summary remains the durable record.

```bash
cliban-flow ticket integrate <KEY> --dry-run
cliban-flow ticket integrate <KEY>
```

The dispatcher accepts only strict ancestry: the tested ticket tree already contains the exact milestone tip it will land on. Integration is therefore a squash with no new combination of trees, so no post-integration build is needed. Do not relax the ancestry guard without also restoring a post-integration build and test gate; those two guarantees are one design.

The resulting squash has no ticket-side merge parent. Compare the agent's reported ready SHA with the SHA recorded by the dispatcher before integration; if they differ, a late commit exists and the ticket must be synced, verified, readied, and integrated again rather than copied in by hand.

## 5. Sweep lessons, then finalize

When every issue is `done` and the milestone branch is green, sweep before the knowledge evaporates.

Each ticket already swept its own inside `complete-issue`, so what you add is what **no single ticket could see**: cross-ticket conflicts, hazards that fired during integration, invariants you enforced by hand. Skim project `## Notes` first and don't restate what's there.

Re-read what the tickets recorded (`cliban issue cat <KEY> --section activity`), keep only what outlives this milestone, and promote each survivor search-first — `cliban project search`, then update the `###` that covers it or add a new one. A milestone that taught nothing durable sweeps to zero; that's a valid outcome.

Then **stop** and hand off, presenting finish/PR/discard against `main`. Landing the milestone branch is the user's call — especially when a phase is a cutover that deletes or replaces existing code. With approval, land it through:

```bash
cliban-flow milestone finish "<milestone name>" --project <KEY>
```

## Parallel-integration hazards

Wave tickets are written against the *same* base in parallel, so they collide on whatever is shared. The orchestrator is the serialization point for every shared resource — and the conflicts that matter most are the ones git does **not** mark.

**1. Clean integration requires strict ancestry.** Two tickets can take divergent designs on the same files and still combine incoherently. `ticket sync` moves that combination and its conflict resolution into the ticket worktree, where the implementer builds, tests, and readies the exact tree. `ticket integrate` may skip a post-integration build only because its strict ancestry guard proves the squash introduces no untested combination; relaxing that guard breaks this guarantee.

**2. Agents commit after they report.** An agent, or a subagent it spawned, can land a commit after `ticket ready`. Squash creates no ticket-side parent to inspect later, so the ready SHA is the immutable handoff: compare it before integration, and make any changed branch repeat sync, verification, and ready.

**3. Serialized shared sequences** (changelog IDs, version files, shared enums, registries). Every agent mints against the *stale* base, so they collide on merge. Pre-assigning reserved IDs helps but merge order still wins — the orchestrator owns the sequence and renumbers at integration, in order, gapless. Tell agents not to bump a shared version file at all; the milestone is one unreleased version until finalize.

Test-to-ticket citations are *not* such a sequence: each agent's key was allocated when its ticket was created, so siblings cite different keys by construction.

**4. Path-based pre-commit hooks don't enforce completeness.** A hook firing on "any file under X changed" passes when an agent does *half* a paired change — the section but not the changelog entry, the keyword but not the grammar. Check the both-halves invariant at integration.

**5. Shared mutable state contention.** Subagents racing on a shared DB or scratchpad can overwrite each other's ticket descriptions. Verify each ticket's description still matches its key before relying on it; keep per-ticket state in the worktree.

**6. Finished agents resume after integration.** A late agent can change a branch the dispatcher already integrated. Treat the ready SHA as the end of that agent's authority and stop its session before integrating.
