# Review Discipline

Non-trivial or risky work gets one fresh-context review over the complete ticket
diff once by default, immediately before ready. Add a mid-ticket checkpoint only
where a mistaken foundation compounds expensively; review the cumulative diff at
that point and do not continue with a Spec failure or serious finding open.

Use the reviewer bound by `docs/agents/issue-tracker.md`, or a general-purpose
agent when none is bound. Every dispatch — checkpoint, pass 2, or re-review
after a fix — runs in fresh context: it reviews the work product, not the
dispatcher's reasoning about it, so it never inherits session history and never
resumes an earlier reviewer. Give it the Spec, plan, implementer's claims, base
and head SHAs, tests, and the project notes matching the ticket (a reviewer
without the repo's known traps re-derives them or misses them) — never the
dispatcher's own theory of where the defect is — and this brief:

> Read the code and `git diff BASE..HEAD`; do not trust the report. Return
> `SPEC: ACCEPT|REJECT` for missing, extra, or misunderstood requirements, then
> `QUALITY:` findings ordered Critical, Important, Minor with file:line evidence.
> Check correctness, edge cases, verification quality, unnecessary complexity,
> and scope drift. Log the verdict and findings summary before sending the full review
> to the supplied agentId. An agent type is never an address. Full review text stays off the board.

Fix and re-review any Spec rejection or Critical/Important finding. Minor-only
results may proceed. If the reviewer is wrong, answer with executable evidence
and log the disposition.

## Dispatched pass 2

Before sync, the implementer reports confidence, a `review: skip | run`
recommendation, one-line evidence, and merge risk. Either side may request
review; the orchestrator makes the final decision at every confidence level.
When it skips, it records `review waived by orchestrator: <reason>` itself: the
gate refuses one logged as `agent:<KEY>`. Standalone
work may use the same vocabulary without an orchestrator waiver.

A verdict relayed by the orchestrator counts, so record it as a verdict rather
than reaching for the waiver — a waiver logged over a review that did happen
records the opposite of the truth on the durable board. On a re-ready after the
branch moved, the verdict line must also carry the new tip SHA, since the old
evidence describes a tree that no longer exists.

A checkpoint-free plan gets one cumulative review. When final review is chosen,
it always covers the complete ticket diff.

A fix for a pass-2 finding is re-reviewed before `ticket ready` — a rule, not a
judgment call, binding every fix author, the orchestrator included, and fixes
scoped as "free" or "trivial" alike. A finding about wording, cosmetics, or a
message is never discharged by removing the check that produced it. The
re-review is a fresh dispatch scoped to the fix
plus what it could plausibly have broken — not the whole branch again — and a
fix that bounces at re-review goes to a fresh implementer with the findings as
the brief, not another iteration by the same author. A re-review verdicts the
fix against the recorded findings: new Critical or Important findings outside
the fix's blast radius are new work for the orchestrator, not a wider bounce,
and new Minor findings never bounce.

A sync resolution that changes behavior gets one focused fresh-context review
over the resolution diff. Mechanical combinations need only renewed executable
verification. The assembled milestone review remains separate and mandatory.
