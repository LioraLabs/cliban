---
name: complete-milestone
description: "Orchestrate an entire cliban milestone end-to-end — one agent per ticket, run in dependency waves, each isolated in its own worktree, integrated onto a milestone branch. Use when asked to orchestrate, drive, or complete a whole milestone of dependency-linked issues."
requires_skills: [cliban-workflow]
---

# Complete Milestone

The orchestrator is a conductor, not a coder: it computes the dependency order, dispatches one agent per ticket, gates each on its dependencies, and integrates finished work onto a milestone branch — never `main` — until the user finalizes.

Inside each ticket runs **`complete-issue` in dispatched mode**, which owns its
proportional plan, implementation, executable verification, and committing. This
skill schedules that and integrates the results; it does not restate the rhythm.

**Load first:** invoke `cliban-flow:cliban-workflow` for the contract (status mapping, where each artifact lands) and `cliban:cliban` for CLI mechanics. Neither loads on its own — reach for them with the Skill tool before the first board action.

**Not this skill:** a single issue (`complete-issue`); work not yet sliced into tickets (`scope-milestone`); issues with no shared integration target.

**What the repo must offer:** a ticket's claims must be checkable at a seam
without a human — a pure function a test calls, a CLI a fixture repo runs,
refs a script asserts — and its blast radius must be separable from its
siblings'. Functional-core/imperative-shell is the most reliable way to get
both in a UI codebase, not the only way. Where a shared surface offers no such
seam, don't abandon the workflow: serialize the wave there and expect review
to be the only gate, priced accordingly — untestable shared surface is exactly
where parallel tickets collide (step 1's check).

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
# {"waves":[["PROJ-5"],["PROJ-6","PROJ-8"],["PROJ-7"]], "chains":[…],
#  "collisions":[{"keys":["PROJ-6","PROJ-8"],"path":"src/router.rs"}], …}
```

`waves[0]` is dispatchable now; wave N is safe once 1..N-1 have merged. Re-run after each integration rather than tracking readiness yourself.

`chains` never schedule; they staff. Walk a chain with one implementer, in the
order printed, starting each ticket only at its wave time from the current
milestone tip. A chain is an author-approved `related_to` group, a linear run
of blocking edges, or same-wave tickets predicted to touch one file. The run is
the case that pays: its tickets are serialised anyway and sit on one surface,
so dispatching each one cold buys no parallelism and re-learns the same code
every wave. Reuse context, never ticket
worktrees or branches. Split a chain only when its carrier grows too expensive
to keep (see the sweep) or the tickets prove unrelated.

- **A cycle exits 2** naming the issues — fix the board before orchestrating.
- **Non-empty `external_blocked`** is a stop-and-ask: those issues are gated by work *outside* the milestone, and no amount of wave-finishing frees them.

The blocking graph answers what *may* run concurrently, not what *should*.
`collisions` answers the rest wherever tickets carry a `## Files` prediction:
each entry names one path that several tickets in a single wave expect to
touch, and those tickets are already joined into a chain, so the staffing rule
above serializes them for you. Read the entries anyway. A collision spanning
most of a wave is a slicing problem to take back to `scope-milestone`, not a
staffing one.

A prediction is a prediction, so keep the older check as its cross-check and as
the fallback for tickets carrying no `## Files`: read each ticket's Spec for the
surface it names, and ask the repo's history which files those surfaces change
together (`git log --format= --name-only -- <paths>`). Name any surviving
overlap in each brief. The evidence for the cost: one wave partitioned along a
language boundary went 8/8 clean; a sibling wave with no blocking edges and two
shared files went 3/3 rejected at triple exploration and review cost. When an
executor finds its prediction wrong it amends the section, and you relay that
to running siblings exactly as `--invariants` relays anything else.

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

The command prints the ticket worktree path. Dispatch one agent per ticket there, parallel within a wave, as **`general-purpose`** — it has to spawn its own implementer and reviewer subagents, which tool-restricted types (`Explore`, `Plan`) cannot do. When the harness supports a per-dispatch model override, dispatch ticket agents on its mid-tier coding model (Claude Code: `model: sonnet`) — ticket execution is the dominant token cost and doesn't need the orchestrator's tier. Reviewers and re-reviewers stay on the session model, deliberately: a review that misses a destructive defect costs more than any model delta.
Give each dispatch the task name derived from its ticket key by lowercasing and
replacing `-` with `_` (`CLI-95` becomes `cli_95`). Keep the returned agent ID;
liveness checks address that ID, not an agent type.

The brief:

1. `cd` into the worktree, confirm isolation, then run `complete-issue` in **dispatched mode** for `<KEY>`.
2. Commit on `<ticket-branch>`, follow dispatched completion through `ticket sync` and `ticket ready`, then report. Do not integrate, move the issue to `done`, or touch `main` or the milestone branch.
3. Report only after `ticket ready` succeeds. Include its immutable SHA, branch, test status, one-line summary, any `## Spec` amendment, and merge-risk notes. Never commit after ready; a changed branch must repeat sync, verification, and ready.

Never pre-plan a ticket for its agent. The agent plans and executes it, adding a
checkpoint only where a mistaken foundation would compound expensively. Do add
wave-level pointers to the brief — what the wave's siblings touch, traps already
in the INTEGRATED ledger — since the ticket-scoped prime from `ticket start`
cannot see across the wave.

Before ready, the implementer sends confidence, a skip/run recommendation,
one-line evidence, and merge risk. Either side may request review; the
orchestrator makes the final decision at every confidence level. If it skips,
record `review waived by orchestrator: <reason>` on the ticket yourself so ready
can proceed — ready refuses a waiver logged as `agent:<KEY>`, since asking the
ticket's agent to write one is asking it to certify its own work. Confidence
informs but never binds this decision.

When it reviews instead, dispatch with the template in
[review.md](../complete-issue/references/review.md) — never an improvised
brief — and have the verdict logged on the ticket. `ticket ready` greps the
Activity Log for either the waiver line above or one line carrying `review`,
`SPEC: ACCEPT`, and `QUALITY: pass` with no `Critical` or `Important` on it
(`pass-2 review verdict: SPEC: ACCEPT, QUALITY: pass` is the canonical shape).
The template's verdict satisfies the gate by construction; a reworded one is
refused with the ticket genuinely reviewed.

After a rejected review, never resume the author — by then the findings are
the brief, and a fresh implementer starts from them plus any handoff. One
exception: a fix the orchestrator can name from the finding alone (exact file,
function, and edit, no re-exploration) it may apply itself in the ticket
worktree, logged on the ticket and gated by the same fresh re-review as any
fix; the first re-review bounce sends it to a fresh agent.

## 4. Sweep running work

Periodically run a cheap, read-only liveness sweep over running and dead agents:

```bash
cliban issue cat <KEY> --section plan
cliban issue cat <KEY> --section activity
git log <base>..<ticket-branch>
git -C <ticket-worktree> status -s
```

A healthy branch has a plan on the board and, over time, advancing activity or
commits. Structured plans may also have ticked steps. The status output exposes uncommitted work. Do not interrupt
a working agent to perform the sweep. If all signals are empty or
stale, ask the agent for its current phase and blocker before concluding it is
stuck; a hard ticket can legitimately stay silent for a long stretch.

Cost is a sweep signal alongside liveness, and it is yours to measure: an agent
cannot police its own growth, and asking it to costs a turn. Every turn re-sends
the whole conversation, so the number that predicts the next turn's price is
**current context**, not spend so far. Read it from the transcript path the
dispatch returned, without pulling any of it into your own context:

```bash
grep -o '"cache_read_input_tokens":[0-9]*' <agent-output-file> | tail -1
```

Past roughly **200k on a solo ticket** or **350k on a chain carrier**, order it
to commit, write the handoff (shape: `complete-issue`'s Work step), and exit;
resume the chain on a fresh agent from that handoff. The observed fresh restart
finished the same merge at 15% of the runaway's cost, because the handoff had
externalized the comprehension. Never read the transcript file itself: it is the
agent's full JSONL and it will swamp you.

Ask through the agent runtime's `send_message` operation using the agent ID saved at dispatch. If that address is unreachable, apply `complete-issue`'s **Resume exception** before declaring the claimant gone, then use `recover-milestone`'s ticket interpretations: respawn work-bearing tickets in their worktree; for an empty ticket run `cliban issue release <KEY>` and redispatch through `ticket start <KEY>`; for an external blocker run `cliban issue mv <KEY> blocked --note "<why>"` and surface it to the user.

Independent siblings continue while dependents wait. After a second death on the same ticket, stop retrying and ask the user.

## 5. Integrate as each agent finishes

The orchestrator integrates, not the agent. **A "done" notification is a claim to verify, not a fact.** Confirm the issue is `in-review` and the report includes the SHA returned by `ticket ready`, then:

Stranded reviews are expected when direct agent delivery fails. Relay the full
review to the ticket agent as part of integration; the ticket's concise verdict
and findings summary remains the durable record.

```bash
cliban-flow ticket integrate <KEY> --dry-run
cliban-flow ticket integrate <KEY> --invariants "<what siblings could break without a conflict marker>"
```

`--invariants` is the integration relay, and writing it is the orchestrator's job — it is the only participant that has seen both sides. Relay what a sibling could break with no conflict marker: signature shapes, load-bearing attributes, what a key actually keys, guard ordering. Not the diff — sync surfaces that on its own. Every later `ticket sync` replays the ledger to its agent on stderr.

The dispatcher accepts only strict ancestry: the tested ticket tree already contains the exact milestone tip it will land on. Integration is therefore a squash with no new combination of trees, so no post-integration build is needed. Do not relax the ancestry guard without also restoring a post-integration build and test gate; those two guarantees are one design.

The resulting squash has no ticket-side merge parent. Compare the agent's reported ready SHA with the SHA recorded by the dispatcher before integration; if they differ, a late commit exists and the ticket must be synced, verified, readied, and integrated again rather than copied in by hand.

## 6. Accept the assembled milestone, sweep, then finalize

The fresh assembled milestone review remains mandatory regardless of per-ticket
waivers.

When every issue is `done` and the milestone branch is green, fold any reported,
milestone-relevant `## Spec` amendments into the milestone description. Then
dispatch one fresh-context reviewer over the assembled milestone branch. Give it
the milestone description (including out-of-scope decisions), the ticket specs,
and the base and milestone-tip SHAs; ask it to verify the promised behavior in
the code and tests and return `ACCEPT` or `REJECT` with terse findings. It must
record that verdict first with `cliban milestone log`. Do not offer finalize until the reviewer passes.

Then sweep before the knowledge evaporates.

Each ticket already swept its own inside `complete-issue`, so what you add is what **no single ticket could see**: cross-ticket conflicts, hazards that fired during integration, invariants you enforced by hand. Skim project `## Notes` first and don't restate what's there.

Re-read what the tickets recorded (`cliban issue cat <KEY> --section activity`), keep only what outlives this milestone, and promote each survivor search-first — `cliban project search`, then update the `###` that covers it or add a new one. A milestone that taught nothing durable sweeps to zero; that's a valid outcome.

Then **stop** and hand off, presenting finish/PR/discard against `main`. Landing the milestone branch is the user's call — especially when a phase is a cutover that deletes or replaces existing code. With approval, land it through:

```bash
cliban-flow milestone finish "<milestone name>" --project <KEY>
```

If the human chooses discard instead, run `cliban-flow milestone abandon "<milestone name>" --project <KEY> --confirm "<why>"`.

## Parallel-integration hazards

Wave tickets are written against the *same* base in parallel, so they collide on whatever is shared. The orchestrator is the serialization point for every shared resource — and the conflicts that matter most are the ones git does **not** mark.

**1. Shared function signatures.** Two tickets extending the same function make incompatible shape decisions — one adds a sixth positional parameter while the other collapses the tail into an options object. The type checker flags it loudly, but the resolution is a design decision spanning both tickets that no single merge resolver can make; the orchestrator decides the shape and relays it.

**2. The same helper invented twice.** Two tickets independently write the same wrapper with different shapes. Neither diff conflicts; the duplication is only visible with both in one tree. At integration, scan the incoming diff for new helpers that duplicate what a sibling already landed.

**3. Serialized shared sequences** (changelog IDs, shared enums, registries). Merge order decides their final order, so put any renumbering through a dispatched ticket and its normal verification gate. Tell agents not to bump a shared version file; the milestone is one unreleased version until finalize.

**4. Path-based pre-commit hooks don't enforce completeness.** A hook firing on "any file under X changed" passes when an agent does *half* a paired change — the section but not the changelog entry, the keyword but not the grammar. Check the both-halves invariant at integration.

**5. Shared mutable state contention.** Subagents racing on a shared DB or scratchpad can overwrite each other's ticket descriptions. Verify each ticket's description still matches its key before relying on it; keep per-ticket state in the worktree.
