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

## Seams

A **seam** is the public boundary a test observes behavior through without
reaching inside. Tests live at seams; code behind one can be rewritten entirely
and the test still holds. Every task names the seam it tests at, and the plan
carries that name before the task is dispatched — a test written at a seam
nobody agreed to is how a suite ends up asserting the implementation back to
itself. When the seam's own shape is the question, that is a design decision for
the plan, not something an implementer settles mid-task.

Work in vertical slices: one test, one implementation, repeat. Writing every
test first tests imagined behavior and freezes the test structure before the
implementation has taught anything.

## Evidence that holds

Test-first is the default for behavior changes, and it is checked, not trusted:
the verifier reverts your implementation, keeps your tests, and requires them to
fail. A test that passes against untouched code was written after the fact or
asserts the implementation back to itself — either way it never proved it can
catch the bug, and no green gate and no diff will say so. If the proposed test
passes on the untouched code, either the behavior already exists or the check
observes the wrong seam. A compile error or broken fixture is setup failure, not
evidence of missing behavior — and a revert that will not compile is a flip
check that proves nothing, so keep the test's seam wide enough to survive one. A test must fail if the behavior breaks: for code that
converges or reconciles, that means feeding a pass its own output — one pass
proves nothing about a fixpoint.

Keep checks outcome-focused. Avoid mocks of code you own, expectations derived
by repeating the production algorithm, assertions that only prove a helper was
called, and tautologies that recompute the expected value the way the code does
— an expected value comes from a known-good literal, a worked example, or the
Spec. Substitute only true system boundaries such as time, randomness, external
APIs, or impractical storage. An assertion inside a callback handed to code that
catches exceptions is swallowed and proves nothing; assert on state observed
after the call returns. When no executable seam exists, a manual check is
acceptable only when disclosed as one — state what was done by hand and what it
could not observe.

When the Spec and executable evidence disagree, amend the Spec and log the
decision. Do not quietly reshape the evidence around the implementation.
