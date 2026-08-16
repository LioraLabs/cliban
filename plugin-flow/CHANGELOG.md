# Changelog

## 0.12.0

- Rebuild `complete-issue` as a **plan → execute loop** with three roles instead of one prose page. The planner (session model) plans and dispatches; an implementer (mid-tier) writes one task; a fresh verifier (mid-tier) re-runs the gate and reads the range against the brief, returning `PASS` or `FAIL` — mechanically, never judgment. New contract bindings: Implementer and Verifier alongside Reviewer.
- Buy judgment at **planner-defined review seams** rather than per task or once at the end. The plan marks the seams before any dispatch; the end of the ticket is always one; everywhere else the verifier is the gate. Reviews run two axes — Spec and Standards — in parallel and are never merged, so one cannot mask the other.
- Stop dispatching milestone ticket agents on the mid-tier model (0.7.0). The orchestrator dispatches a planner on the session model; the demotion happens one level lower, where it belongs.
- Give the orchestrator a **wave-boundary integration review**: when a wave's last ticket integrates, review the assembled wave for what git does not mark, before the next wave branches off that tip. A wave of one skips it — no siblings, nothing to collide with.
- Thin the orchestrator: everything about how a ticket gets done now lives in `complete-issue`, and ticket-state interpretation has one owner in `recover-milestone`.
- Escalation ladder instead of a flat retry: a failed verifier goes to a fresh implementer, the second failure to a fresh implementer one tier up, the third is three strikes. A second review bounce ends with the planner adjudicating and logging `Ruling: <what> — <why> — <cost if wrong>`. The loop runs continuously between tasks, stopping only for a destructive operation, a security-sensitive action, an effect outside the worktree, or a plan defect no reading resolves.
- Delete `chains` from `milestone waves` entirely — the JSON field, the table line, the `related_to` grouping, the linear-run inference, and every mention in the skills. Chains existed so one agent could pay comprehension once across related tickets; an agent that delegates its reading has little comprehension to amortise, and a chain carrier's accumulated context is the thing the orchestrator then has to cap. Collisions fed those chains too, and since chains were a connected-component walk the joining was transitive — one hub path (`lib.rs`, `Cargo.toml`, a registry) collapsed a whole wave onto one agent. **Blocking edges are now the only thing that orders work**; collisions are reported for the orchestrator to brief its agents with, and the wave review catches what survives.
- Give the verifier a **flip check**: revert the range's implementation, keep its tests, require them to fail, restore, require green. A green gate and an in-brief diff are both satisfied by tests written after the fact and by tautologies, so without it the seam economy rested on a gate that could not see the one failure mode it exists to catch.
- Run the focused gate once at ticket start and log `baseline: green (<command>)`. A red baseline made the first task's failure ambiguous and burned the three-strikes count on a break the ticket did not cause.
- Tell the planner and the orchestrator that the board outranks their memory, after a compaction especially. This completes the token-rule deletion: delegation made context loss survivable, this makes surviving it automatic.
- Carry accepted Minors forward instead of dropping them: the final seam's brief names every Minor logged at an earlier one, and anything outliving the ticket — an accepted Minor, a `Ruling:` with a real cost, a trap for the next ticket — goes to `cliban milestone log`, which the milestone reviewer now reads.
- Add a spike branch to `explore-feature` for when the open question is "does this shape work in code", and decision tickets to `scope-milestone` for a frontier that will not empty in one sitting — `decision`-labeled issues with real edges, so `issue ls --ready` is the live frontier.
- Read the repo's ADRs and domain docs: the planner at start, and every implementer brief carries the ADRs governing the files its task touches. The contract already said they live in the repo; no skill ever opened them.
- Seams are asymmetric after planning: evidence arriving mid-loop may add one, nothing may remove one.
- Delete every token-capping rule: the orchestrator's transcript cost probe, the ~200k/~350k exit thresholds, the "sizing is the biggest cost lever" slicing rationale, and the context-window framing in `scope-milestone`. Asking an agent to measure and cap its own or another's context never worked; the loop makes it structural instead, since the planner delegates its reading and the implementers are throwaway. Slices are now sized by deliverable, and an agent still writes a handoff and exits when it strikes out or is called off.
- Delete the six prose-grep test suites. They pinned wording, not behavior: every rewrite failed them and no defect ever did. What survives is structural — dead links, the installed plugin resolving its dispatcher, dispatcher commands derived from `help`, no raw git in the orchestrator — with skill behavior left to the scenario suite.

## 0.11.0

- Rewrite every workflow skill around its happy path: the decided rules stay (the test suite pins them), the battle-story rationale moves to git history. 30% fewer words across the set.
- Name complete-issue's dead-claimant takeover the **Resume exception** — the name complete-milestone and recover-milestone were already citing without a referent.
- Lower the prose-diet cap to 1000 lines and complete-issue's cap to 86 to lock the diet in.
- Replace unobservable spin guards with countable stop conditions: three strikes on the same failing check, acceptance criteria as the finish line, `## Files` as the executor's exploration leash, a two-bounce cap on review ping-pong with re-reviews verdicting only the recorded findings, and the liveness sweep tied to every orchestrator wake instead of "periodically". Drop the self-measured ~200k context exit an agent can't observe; the orchestrator's cost probe orders that exit.

## 0.10.0

- Staff a chain with one implementer by default, and say why: a linear run of blocking edges is serialised anyway, so dispatching each ticket cold buys no parallelism and re-learns the same surface every wave.
- Give the liveness sweep a cost probe the orchestrator can actually run, over the transcript path the dispatch returns, with thresholds at 200k solo and 350k for a chain carrier.
- Correct the cost metric in both executor skills from cumulative spend to current context, since every turn re-sends the conversation.

## 0.9.0

- Refuse a `ticket ready` review waiver written by the ticket's own agent: authorship is read from the activity feed's actor, not matched by line shape, so a ticket can no longer certify its own work as `agent:<KEY>`.
- Name both accepted forms in the review refusal, so a genuine relayed verdict is recorded as a verdict instead of a false waiver.

## 0.8.0

- Carry the Skill remediation milestone: collision prediction, the pass-2 contract, the handoff shape, the integration relay, and note priming.

## 0.7.0

- Make token cost an explicit lever in complete-issue: batch tool calls, chain shell steps, don't re-read settled files.
- Dispatch milestone ticket agents on the mid-tier coding model where the harness supports a per-dispatch override.
- Size scoped slices to roughly 100–150 tool calls, since an executor's per-turn cost grows with its accumulated context.

## 0.6.0

- Make the dispatcher own standalone and dispatched ticket start/ready state.
- Replace implementation ceremony with a proportional, porous lean lifecycle.
- Keep one fresh-context review by default while leaving dispatched pass-two review discretionary.
