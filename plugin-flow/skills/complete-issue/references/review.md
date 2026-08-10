# Checkpoint Review — the reference for the gates in step 6 and step 7

Review is **batched to the plan's checkpoints**, not run after every task. N tasks
cost **one** review at their checkpoint, not N. Reviewing after every task is what
makes a ticket crawl; the checkpoints put the gate where a bug would otherwise
compound — typically right before later tasks stack on a foundational slice.

Not every ticket earns a gate, either. A plan with no `### Review Checkpoint`
marker has exactly one gate: the end of the plan. That is a legitimate shape for
a small ticket, and choosing it is the planner's call at step 5.

## Who reviews

The adapter (`docs/agents/issue-tracker.md`) binds the reviewer — a skill, an
agent type, or nothing. Read it before the first gate.

- **A bound skill or agent** — use it, and give it the two verdicts below as its
  brief. A review skill that already has its own rubric keeps it; the spec
  verdict is what you add, because only this workflow knows what the ticket asked
  for.
- **Nothing bound** — dispatch a general-purpose agent with the template below.

Either way the reviewer runs in **fresh context**. It must never inherit this
session's history: it reviews the work product, not your reasoning about it. Pass
it the task text and the diff, and let it read the code itself.

## The two verdicts, in one dispatch

Spec compliance and code quality come back from a single reviewer pass. Two
dispatches per gate is the tax this design exists to avoid.

```
Dispatch a fresh-context reviewer:

  You are reviewing a batch of completed work at a review checkpoint.

  ## What was requested

  <full text of every task in this checkpoint's group, from the issue's ## Plan>

  ## What the ticket promised

  <the issue's ## Spec — the claims this work is supposed to make true>

  ## What the implementer claims

  <the implementer's report(s) for the group>

  ## The diff to review

  BASE_SHA: <HEAD at the previous checkpoint, or the branch base for the first>
  HEAD_SHA: <current HEAD>

  Review `git diff BASE_SHA..HEAD_SHA` plus the tests.

  ## Do not trust the reports

  A report may be incomplete, optimistic, or wrong. Verify by reading the code
  and the tests, never by trusting a claim.

  ## Verdict 1 — spec compliance, per task

  Reading the code, not the report:
  - Missing — requested but not implemented, or claimed but absent?
  - Extra — built but never requested? Over-engineering counts.
  - Misunderstood — right feature, wrong approach, or the wrong problem solved?
  - Uncited — behavior a `feature` or `bug` ticket shipped with no test citing
    the ticket (see tdd.md).

  Per task: ✅ compliant, or ❌ with specific file:line issues.

  ## Verdict 2 — code quality, across the group's diff

  - Idiomatic style, naming, subtle bugs, unhandled edge cases
  - Test coverage gaps, and tests that pass without proving anything
    (tdd.md's anti-patterns are the checklist)
  - File organization: one clear responsibility per file, independently
    testable units
  - Files this change created or significantly grew — not pre-existing size

  Report every issue, including ones you are unsure about. The severity label
  does the filtering, not omission.

  ## Output

  SPEC: the per-task ✅/❌ list.
  QUALITY: Strengths, then Issues as Critical / Important / Minor each with
  file:line, then an Assessment.
```

## Acting on the verdict

| Result | Action |
|---|---|
| Any spec ❌, or any Critical/Important quality issue | Fix with specifics, then **re-review this checkpoint**. Do not advance past a gate with these open. |
| Only Minor | Accept and continue. `cliban issue log` them if they accumulate across checkpoints — a pile of Minors is a finding in itself. |
| Reviewer is wrong | Push back with the code or test that proves it, and say so in the log. A review is evidence, not a verdict to obey. |

Log the outcome either way — `cliban issue log <KEY> "checkpoint <scope>: <what
the review found and what you did>"`. The gate having *run* is part of the
ticket's history; a silent gate is indistinguishable from a skipped one.

## The final gate

After the last task, one cumulative review over the whole ticket's diff. It is
lighter than a checkpoint — most issues are already caught — and it looks for
what a per-checkpoint view structurally cannot see:

- Cross-checkpoint inconsistency: two groups that each passed but disagree
- Architectural drift from what the `## Spec` described
- Dead code, orphaned helpers, abandoned intermediate states
- Scope that grew past what the ticket asked for, accumulated a slice at a time

On a plan with no checkpoint markers, this **is** the only gate, and it is not
optional.
