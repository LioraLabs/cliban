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

## 2. Create the integration branch

```bash
ROOT=$(git rev-parse --show-toplevel)
SLUG=<milestone-slug>
git -C "$ROOT" branch "milestone/$SLUG" main 2>/dev/null || true
```

## 3. Per ready ticket — worktree + one agent

Create each worktree **at wave time**, off the *current* milestone branch, never all up front — otherwise dependent tickets branch off code that lacks their dependency's work.

```bash
git worktree add "$ROOT/.worktrees/<ticket-branch>" -b "<ticket-branch>" "milestone/$SLUG"
```

Use the issue's `git_branch_name`. Then dispatch one agent per ticket, parallel within a wave, as **`general-purpose`** — it has to spawn its own implementer and reviewer subagents, which tool-restricted types (`Explore`, `Plan`) cannot do.

The brief:

1. `cd` into the worktree, confirm isolation, then run `complete-issue` in **dispatched mode** for `<KEY>`.
2. Commit on `<ticket-branch>` and report. Do not merge, move the issue to `done`, or touch `main` or the milestone branch.
3. Report only after every commit has landed — never with staged-but-uncommitted work, never commit after reporting. Include: final commit SHA, branch, test status, one-line summary, any `## Spec` amendment, and merge-risk notes.

Never pre-plan a ticket for its agent. The agent runs plan and execute itself; that's where the per-ticket review checkpoints live.

## 4. Integrate as each agent finishes

The orchestrator integrates, not the agent. **A "done" notification is a claim to verify, not a fact.**

```bash
cd "$ROOT"
# Verify the agent actually finished, before trusting the report:
git -C ".worktrees/<ticket-branch>" log --oneline "milestone/$SLUG..<ticket-branch>"  # must have commits
git -C ".worktrees/<ticket-branch>" status -s     # staged-but-uncommitted = NOT done
#   Also confirm the plan's checkboxes are ticked. Work staged or tasks open means
#   the agent came to rest early — resume it (SendMessage) to finish and commit.

git checkout "milestone/$SLUG"
git merge --no-ff "<ticket-branch>"    # resolve conflicts here, in the orchestrator
<build the project>                     # BUILD FIRST — hazard 1
<run the full test suite>               # milestone must stay green

# Did the merge capture the branch's current tip? (hazard 2)
test "$(git rev-parse <ticket-branch>)" = "$(git rev-parse HEAD^2)" \
  || echo "LATE COMMIT — cherry-pick it in"

cliban issue mv <KEY> done
cliban issue log <KEY> "merged to milestone/$SLUG as $(git rev-parse --short HEAD)"
cliban linear push <KEY> || true        # only if the ticket came from Linear; never fatal
git worktree remove "$ROOT/.worktrees/<ticket-branch>"
git branch -d "<ticket-branch>" || git branch -D "<ticket-branch>"   # -d fails after a conflict-resolved --no-ff
```

Build or tests failing on the merge result means the ticket is not done — fix it here (the break is usually cross-ticket) or reopen it before proceeding.

## 5. Sweep lessons, then finalize

When every issue is `done` and the milestone branch is green, sweep before the knowledge evaporates.

Each ticket already swept its own inside `complete-issue`, so what you add is what **no single ticket could see**: cross-ticket conflicts, hazards that fired during integration, invariants you enforced by hand. Skim project `## Notes` first and don't restate what's there.

Re-read what the tickets recorded (`cliban issue cat <KEY> --section activity`), keep only what outlives this milestone, and promote each survivor search-first — `cliban project search`, then update the `###` that covers it or add a new one. A milestone that taught nothing durable sweeps to zero; that's a valid outcome.

Then **stop** and hand off, presenting merge/PR/discard against `main`. Landing the milestone branch is the user's call — especially when a phase is a cutover that deletes or replaces existing code.

## Parallel-integration hazards

Wave tickets are written against the *same* base in parallel, so they collide on whatever is shared. The orchestrator is the serialization point for every shared resource — and the conflicts that matter most are the ones git does **not** mark.

**1. Clean auto-merge ≠ coherent code.** Two tickets that took divergent designs on the same files auto-merge with zero conflict markers yet produce a tree that doesn't compile. Build after every merge, even marker-free ones: the compiler is the authority, git silence is not. The fix is usually porting an *already-merged* ticket's code onto the newer ticket's API, not reverting.

**2. Agents commit after they report.** An agent, or a subagent it spawned, can land a commit after its "done" notification. Verify `<branch>` equals the merge's ticket-side parent (`HEAD^2`) before deleting anything; cherry-pick stragglers.

**3. Serialized shared sequences** (changelog IDs, version files, shared enums, registries). Every agent mints against the *stale* base, so they collide on merge. Pre-assigning reserved IDs helps but merge order still wins — the orchestrator owns the sequence and renumbers at integration, in order, gapless. Tell agents not to bump a shared version file at all; the milestone is one unreleased version until finalize.

Test-to-ticket citations are *not* such a sequence: each agent's key was allocated when its ticket was created, so siblings cite different keys by construction.

**4. Path-based pre-commit hooks don't enforce completeness.** A hook firing on "any file under X changed" passes when an agent does *half* a paired change — the section but not the changelog entry, the keyword but not the grammar. Check the both-halves invariant at integration.

**5. Shared mutable state contention.** Subagents racing on a shared DB or scratchpad can overwrite each other's ticket descriptions. Verify each ticket's description still matches its key before relying on it; keep per-ticket state in the worktree.

**6. Finished agents re-create branches and worktrees.** A late rebase can resurrect something you already cleaned up. Re-scan `git worktree list` before finalizing — but never remove the live worktree of a still-running agent.
