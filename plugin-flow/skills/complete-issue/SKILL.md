---
name: complete-issue
description: "Take one cliban issue from start through planning, work, proof, and handoff."
requires_skills: [cliban-workflow]
---

# Complete Issue

One ticket, end to end. You are its **planner**: you write the plan, dispatch
every change to an implementer, gate each task with a mechanical verifier, buy
judgment only at the seams you drew, and hand off. Load
`cliban-flow:cliban-workflow` and `cliban:cliban` first — the dispatcher owns
git and board transitions in both modes. The session-start hook surfaces
`in-review` candidates: when git or the PR proves one merged, run
`mv <KEY> done --note "merged as <sha>"`.

| Role | Model | Does |
|---|---|---|
| planner (you) | session | plans, dispatches, adjudicates, syncs, readies |
| implementer | mid-tier | one task: writes it, tests it, commits it, reports |
| verifier | mid-tier | re-runs the gate, diffs the range against the brief, `PASS` or `FAIL` |
| seam reviewer | session | judgment over the diff since the last seam |

## Start

Run `cliban-flow ticket start <KEY>`. Standalone work starts from `main`;
dispatched work starts from the current milestone tip — first export
`CLIBAN_ACTOR=agent:<KEY>`, use the supplied worktree, and leave `main`, the
milestone branch, integration, and the move to done to the orchestrator.

**Resume exception** — a claimed in-progress ticket that may belong to a dead
session: derive its position from `## Plan`, `## Activity Log`, the worktree,
and git. Take over only when your orchestrator or the user confirms the
claimant is gone (`issue release <KEY>` or `claim <KEY> --force`), then resume
the existing artifacts — fresh session, warm workspace.

Read the issue, its Spec and activity, the milestone description, the adapter,
the project notes `ticket start` printed on stderr, the ADRs and domain docs
the repo git-tracks where the ticket's area has them, and the code.

Then run the focused gate once, before planning, and log
`baseline: green (<command>)`. A red baseline is not yours to absorb: block on
it, or log what is already failing and why the ticket proceeds anyway.
Otherwise the first task's failure is ambiguous and the strike count burns on a
break the ticket did not cause.

## Plan

Write a proportional `## Plan` before implementation and confirm it with
`issue cat` before execution begins — a sentence and one task for small work,
ordered `### Task N:` headings with checkboxes for large. Never replace the
whole description: the board-visible plan is the recoverability guarantee.

Every task carries what a fresh implementer needs and nothing else:

| Field | Why |
|---|---|
| Outcome | what observably changes — slice by outcome, never by file |
| Files | the leash, from `## Files` |
| Seam under test | the public boundary its tests observe ([verification.md](references/verification.md)) |
| Evidence | the check that fails when the outcome is absent |
| Publishes | signatures and invariants later tasks consume |

Then mark the **review seams**: the points where a wrong foundation would
compound expensively, and the end of the ticket, which is always one. Seams are
where you spend judgment; everywhere else the verifier's gate is the check.
Small work has exactly one seam. Marking a seam at every task means the plan
does not know where its risk is. The marking is asymmetric afterwards: evidence
arriving mid-loop may add a seam, nothing may remove one — a plan written
against an unfamiliar codebase is exactly where the risk model is worst.

## The loop

Per task, in plan order, one at a time — implementers share a worktree and
would collide. Each dispatch ends your turn; the delegate's completion wakes
you with its report. The plan and log on the board are the loop's state,
re-derived on every wake.

1. **Dispatch the implementer** (contract Implementer binding) with the worktree
   path, ticket key, the Spec excerpt the task must satisfy, the task's five
   fields, the interfaces earlier tasks published, any ADR governing the files
   it touches, and the disciplines it applies —
   [verification.md](references/verification.md) plus the
   installed skills relevant to this ticket. Its bounds: commit on
   the ticket branch; report SHA range, evidence, files touched, and what it
   wants reviewed and why; never spawn subagents of its own, and never touch
   the board, the plan, the next task, `main`, or the milestone branch.
2. **Dispatch a fresh verifier** (contract Verifier binding) with the SHA range
   and the same task brief — never the implementer's report, which is the claim
   under test. It runs three checks and returns `PASS` or `FAIL: <one line>`:
   the repository's focused gate is green; the range is the brief and nothing
   else; and the **flip check** — with the range's implementation reverted to
   base and its tests kept, the new or changed tests fail, then pass again once
   restored. A test that passes against untouched code was written after the
   fact or asserts the implementation back to itself, and neither the gate nor a
   diff can see it. Mechanical only; judgment is the reviewer's.
3. **`PASS`** — tick the checkbox and log one line. **`FAIL`** — a fresh
   implementer takes the task with the failure as its brief; the second failure
   escalates to a fresh implementer one tier up. **Three strikes:** the same
   check still failing after three materially different attempts is a finding,
   not a loop — log it, then block with the reason or report to your
   orchestrator.

A change you can name completely before opening the file — a tiny ticket, one
named review finding — you write yourself; it still goes through a verifier,
because your own claim is worth exactly what an implementer's is.

The board outranks your memory, always, and after a compaction especially: the
plan's ticked boxes, the activity log, and `git log` say what happened. Re-read
them rather than re-dispatching a task you cannot remember finishing.

Run the loop to the end without consulting the user: ending your turn on an
in-flight dispatch is the loop running, and the next report resumes it.
Consult the user only for a destructive or irreversible operation, a
security-sensitive action, an effect outside your worktree, or a plan defect
no reading resolves.

The Spec's acceptance criteria are the finish line — work past them is
gold-plating; promote discovered scope, don't absorb it, and amend the Spec when
evidence disproves it. An edit outside `## Files` means stop: amend the section
with why and re-check that the plan still fits one ticket.

## At each seam

Dispatch the review in [review.md](references/review.md) over the diff since
the last seam, and log its verdict. A rejection briefs a fresh implementer —
never the author, never you. A second bounce on the same finding ends the loop:
adjudicate it yourself and log
`Ruling: <what you decided> — <why> — <what it costs if wrong>`, then continue
or block. Nothing crosses a seam with a Spec rejection or a Critical or
Important finding open. Minors may proceed, but a Minor you leave open is
carried, not dropped: the final seam's brief names every Minor logged at an
earlier one, so the last reviewer sees the accumulation rather than one diff's
worth of it.

Before the final seam, run the repository's full build, lint, typecheck, and
test gate — yours to run, an implementer's to fix. A gate that fits a
foreground call runs in one; one that could outlast it goes to a delegate
that runs it and returns the output.

## Handoff

The handoff is the exit ritual, and it is the step to protect above every
other: the ticket's value either leaves the session here or dies with it.
Commit, then sweep the loose ends onto durable ground — anything open that
outlives the ticket (an accepted Minor, a `Ruling:` with a real cost, a trap
the next ticket will hit) goes to `cliban milestone log` as one line, because
the ticket's log dies with the ticket's review and the milestone reviewer
reads the milestone's. Then run `cliban-flow ticket sync <KEY>` and resolve
the conflicts yourself (combining two verified trees is planning, not
implementation), explain each resolution diff, re-run focused and full
verification, and run `cliban-flow ticket ready <KEY>` — its gate reads the
final seam's logged verdict. The ready SHA is the immutable handoff; never
commit after ready.

Then hand in the deliverable and end: your final message reports the ready
SHA, branch, checks, seam verdicts, one-line summary, Spec amendments, merge
risks, `confidence: high | medium | low`, and `review: skip | run` — grounded
in the seam verdicts and in what your implementers asked to have reviewed,
not in a feeling. No approval step exists: your orchestrator decides pass 2
on the ready SHA after you are gone, and its findings arrive as a fresh
delegate's brief on this ticket. Standalone work follows the same primitives,
then offers merge/PR/discard.

When you strike out, or your orchestrator calls you off, commit what stands,
write the handoff, and exit — a fresh planner finishes from the board and the
worktree it inherits. The
handoff is one `issue log` entry: open findings, the half-applied change's exact
boundary, sync state, dead ends, rulings, and disagreements stated.

Sweep one durable lesson into project notes (search first) only if it helps a
future ticket — most teach none. If stuck, block with the external reason or
release the claim.
