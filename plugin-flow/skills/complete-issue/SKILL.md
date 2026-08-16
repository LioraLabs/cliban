---
name: complete-issue
description: "Take one cliban issue from start through planning, work, proof, and handoff."
requires_skills: [cliban-workflow]
---

# Complete Issue

One ticket, end to end. Load `cliban-flow:cliban-workflow` and `cliban:cliban`
first; the dispatcher owns git and board transitions in both modes. The
session-start hook surfaces `in-review` candidates: when git or the PR proves
one merged, run `mv <KEY> done --note "merged as <sha>"`.

## Start

Run `cliban-flow ticket start <KEY>`. Standalone work starts from `main`;
dispatched work starts from the current milestone tip — first export
`CLIBAN_ACTOR=agent:<KEY>`, use the supplied worktree, and leave `main`, the
milestone branch, integration, and the move to done to the orchestrator.

**Resume exception** — a claimed in-progress ticket that may belong to a dead
session: read its `## Plan`, `## Activity Log`, and worktree, then
ask the claimant. Take over only when it cannot continue or the orchestrator confirms
it ended (`issue release <KEY>` or `claim <KEY> --force`), and resume the
existing artifacts.

Read the issue, its Spec and activity, the milestone description, the adapter,
the project notes `ticket start` printed on stderr, and the code.

## Plan

Write a proportional `## Plan` before implementation and confirm it with
`issue cat` before execution begins — a sentence for small work, `### Task N:`
headings with checkboxes for large. Add a mid-ticket review checkpoint only
where a wrong foundation compounds expensively. Never replace the whole
description: the board-visible plan is the recoverability guarantee.

## Work

Inspect the installed skills and apply the disciplines relevant to this
ticket. The Spec's acceptance criteria are the finish line — work past them is
gold-plating; promote discovered scope instead of absorbing it, and amend the
Spec when evidence disproves it. `## Files` is your leash as well as a
prediction: an edit outside it means stop — amend the section with why and
re-check that the plan still fits one ticket; reading far beyond it and its
direct callers is the plan failing, not diligence. **Three strikes:** the same
check still failing after three materially different attempts is a finding,
not a loop — log what you learned, then block with the reason or report to
your orchestrator.

Every turn re-sends the whole conversation: batch independent tool calls,
chain sequential shell steps, don't re-read settled files. Your orchestrator
measures your cost and may order an exit; on that order, or when you strike
out, commit what stands, write the handoff, and exit — a fresh agent finishes
from the board at a fraction of the cost. The handoff is one `issue log`
entry: status per open review finding, the half-applied change's exact
boundary, sync state, dead ends, and disagreements stated. Log discoveries,
dead ends, and decisions — commits are the durable work record.

## Prove

Follow [verification.md](references/verification.md): every meaningful claim
needs executable evidence. Run the focused checks, then the repository's full
build, lint, typecheck, and test gate. For non-trivial or risky work,
standalone mode runs the once-by-default fresh-context review in
[review.md](references/review.md); dispatched mode requests it at Handoff.

## Handoff

Commit, then report `confidence: high | medium | low`, `review: skip | run`,
one-line evidence, and merge risk — no numeric score. In dispatched mode, wait
for the orchestrator's review decision; it records the verdict or waiver. Then
run `cliban-flow ticket sync <KEY>`, resolve the conflicts and explain each
resolution diff, re-run focused and full verification, and run
`cliban-flow ticket ready <KEY>`. The ready SHA is the immutable handoff —
never commit after ready. Standalone work follows the same primitives
without an orchestrator waiver, then offers merge/PR/discard. Dispatched work reports
SHA, branch, checks, summary, Spec amendments, and merge risks.

Sweep one durable lesson into project notes (search first) only if it helps a
future ticket — most teach none. If stuck, block with the external reason or
release the claim.
