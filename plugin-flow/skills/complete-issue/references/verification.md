# Verification Discipline

Every meaningful claim needs executable evidence that would fail when the claim
is false. Prefer the highest existing public seam that observes the result, and
derive expected values independently from the implementation.

| Claim | Evidence |
|---|---|
| Behavior | A failing test first, then the smallest implementation that passes it |
| Bug | A reproduction first, retained as a regression check |
| Performance | A repeatable baseline first, then the same measurement after the change |
| Refactor | Characterization or existing tests proving behavior stayed unchanged |
| Build or config | A real build or integration assertion exercising the changed path |
| Static property | The applicable lint or typecheck assertion |

Test-first is the default for behavior changes. If the proposed test passes on
the untouched code, either the behavior already exists or the check observes the
wrong seam. A compile error or broken fixture is setup failure, not evidence of
missing behavior. A test must fail if the behavior breaks: for code that
converges or reconciles, that means feeding a pass its own output — one pass
proves nothing about a fixpoint.

Keep checks outcome-focused. Avoid mocks of code you own, expectations derived
by repeating the production algorithm, and assertions that only prove a helper
was called. Substitute only true system boundaries such as time, randomness,
external APIs, or impractical storage. An assertion inside a callback handed to
code that catches exceptions is swallowed and proves nothing; assert on state
observed after the call returns. When no executable seam exists, a manual check
is acceptable only when disclosed as one — state what was done by hand and what
it could not observe.

When the Spec and executable evidence disagree, amend the Spec and log the
decision. Do not quietly reshape the evidence around the implementation.
