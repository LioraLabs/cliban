---
name: complete-issue
description: "Take one cliban issue from start through planning, work, proof, and handoff."
requires_skills: [cliban-workflow]
---

# Complete Issue

One ticket, end to end. Load `cliban-flow:cliban-workflow` and `cliban:cliban`
before the first board action. Resolve the dispatcher from the workflow contract;
it owns the git and board transitions in both modes.

The session-start hook surfaces `in-review` candidates. When git or the PR proves
a prior standalone handoff merged, run `mv <KEY> done --note "merged as <sha>"`.
Linear bridge synchronization is a separate explicit action after lifecycle moves.

## Start

Run `cliban-flow ticket start <KEY>`. Standalone work starts from `main`;
milestone-dispatched work starts from the current milestone tip. In dispatched
mode first export `CLIBAN_ACTOR=agent:<KEY>`, use the supplied worktree, and never
integrate, move the issue to done, or touch `main` or the milestone branch.

If a claimed in-progress ticket may belong to a dead session, read its `## Plan`,
`## Activity Log`, and worktree, then ask the claimant. Take over only when it
cannot continue or the orchestrator confirms it ended; use `issue release <KEY>` or
`claim <KEY> --force` and resume the existing artifacts.

Read the issue, its Spec and activity, relevant project notes, the milestone
description, the adapter, and the code before deciding what work is needed.

## Plan

Write a proportional `## Plan` before implementation and confirm it with
`issue cat`: a sentence or short approach is enough for small work; larger work
may use ordered `### Task N:` headings and checkboxes. Add a mid-ticket review
checkpoint only where a wrong foundation compounds expensively. Never replace
the whole description. This board-visible plan is the recoverability guarantee;
confirm it before execution begins.

## Work

Inspect the installed skills and apply the implementation, debugging, language,
and review disciplines relevant to this ticket. Keep this workflow porous: it
defines lifecycle invariants, not how to program. Commits are the durable work
record; choose their cadence. Log only discoveries, dead ends, scope changes,
and decisions. Amend the Spec when evidence disproves it. Promote discovered
scope instead of absorbing it.

## Prove

Follow [verification.md](references/verification.md): every meaningful claim
needs executable evidence. Then run the focused checks and the repository's full
build, lint, typecheck, and test gate that apply. For non-trivial or risky work,
standalone mode runs the once-by-default fresh-context review in
[review.md](references/review.md); dispatched mode requests it at Handoff.

## Handoff

Commit, then report `confidence: high | medium | low`, `review: skip | run`,
one-line evidence, and merge risk; use no numeric score. In dispatched mode the
orchestrator decides pass 2 review and records its verdict or waiver; wait for
that decision. Run `cliban-flow ticket sync <KEY>`, resolve the conflicts and
explain each resolution diff, re-run focused and full verification, then
`cliban-flow ticket ready <KEY>`. Its immutable SHA is the handoff; never commit
after ready. Standalone work follows the same primitives without an orchestrator waiver,
then offers merge/PR/discard. Dispatched work reports
SHA, branch, checks, summary, Spec amendments, and merge risks to its orchestrator.

Finally, search project notes and sweep one durable lesson only if it will help a
future ticket. Most tickets teach none. If stuck, block with the external reason
or release the claim.
