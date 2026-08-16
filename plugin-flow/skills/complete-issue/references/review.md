# Review Discipline

Two tiers, and the difference is what they are allowed to think about. The
**verifier** is mechanical — the gate is green or it is not, the diff is the
brief or it is not, the tests fail against the untouched code or they never
proved anything — and it runs on every task ([complete-issue](../SKILL.md)'s
loop owns it). A **review** is judgment, costs the session model, and runs only
where a planner marked a seam. Buying judgment at every task is how a ticket
gets expensive; buying none is how a wrong foundation reaches integration.

## The seam review

Fresh context, every time: it reviews the work product, not the dispatcher's
reasoning about it, so it never inherits session history and never resumes an
earlier reviewer. Use the reviewer bound by `docs/agents/issue-tracker.md`, or a
general-purpose agent when none is bound, on the session model.

Give it the Spec, the plan, the implementers' claims, the base and head SHAs of
the diff since the last seam, the tests, and the project notes matching the
ticket — a reviewer without the repo's known traps re-derives them or misses
them — but never your own theory of where the defect is. Then two axes, in
parallel, so neither pollutes the other's context:

> **Spec.** Read the code and `git diff BASE..HEAD`; do not trust the report.
> Report requirements missing or partial, behavior nobody asked for, and
> requirements implemented wrongly. Quote the Spec line for each.
> **Standards.** Same diff, against the repo's documented standards, and
> against the smell baseline where it documents none: duplicated logic,
> speculative generality, primitive obsession, shotgun surgery, mysterious
> names. A documented standard overrides the baseline; skip what tooling
> enforces; label each as a judgment call.

Both return `SPEC: ACCEPT|REJECT` then `QUALITY:` findings ordered Critical,
Important, Minor with file:line evidence. Report the axes separately — a change
can pass one and fail the other, and merging them lets one mask the other.

Log the verdict on the ticket before sending the full review to the supplied
agentId; an agent type is never an address, and full review text stays off the
board. The line the ready gate reads:

```
cliban issue log <KEY> "review: SPEC ACCEPT; QUALITY pass — <findings>"
cliban issue log <KEY> "review waived by orchestrator: <reason>"
```

A waiver logged by the ticket's own agent is refused: the orchestrator writes
it, or the review runs. A verdict relayed by the orchestrator is still a
verdict — record it as one. On a re-ready after the branch moved, the verdict
must name the new tip SHA, since the old evidence describes a tree that no
longer exists.

## Fixing what a review found

Every Spec rejection and every Critical or Important finding is fixed and
re-reviewed before the seam is crossed; Minor-only may proceed, carried forward
into the final seam's brief rather than closed. Three Minors on one surface is a
finding the diff that produced each could not show. The fix goes to
a **fresh implementer** with the findings as its brief — never the author,
never the planner, never the orchestrator, and never scoped away as "trivial".
A finding about wording, cosmetics, or a message is never discharged by
deleting the check that produced it.

The re-review is a fresh dispatch scoped to the fix plus what it could
plausibly have broken, not the whole branch again. It verdicts each recorded
finding addressed or not: new Critical or Important findings outside the fix's
blast radius are new work, not a wider bounce, and new Minor findings never
bounce. A second bounce on the same finding ends the loop — the planner
adjudicates and logs a `Ruling:` line rather than dispatching a third round. If
the reviewer is wrong, answer with executable evidence and log the disposition.

## The orchestrator's reviews

**Pass 2.** Before sync the planner reports confidence and a `review: skip |
run` recommendation grounded in the seam verdicts and in what its implementers
asked to have reviewed. Either side may request review; the orchestrator decides
at every confidence level, and a ticket whose final seam review passed is
already reviewed — pass 2 is for what the planner could not see.

**Integration.** At a wave boundary the orchestrator reviews the assembled wave,
not the tickets: the conflicts git does not mark. A wave of one has no siblings
to collide with, so it skips.

**Assembled milestone.** Mandatory regardless of every waiver below it, over the
whole milestone branch, before finalize is offered. Its brief carries the
milestone log — accepted Minors, rulings, and traps the tickets left there are
the residue no per-ticket review was in a position to weigh.

A sync resolution that changes behavior gets one focused fresh-context review
over the resolution diff. Mechanical combinations need only renewed executable
verification.
