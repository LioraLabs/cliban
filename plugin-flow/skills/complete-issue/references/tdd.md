# Test-Driven Development — the reference for step 6

The loop is red → green. This file is what makes that loop produce tests worth
keeping: what a test must cite, what a good test is, and the four ways a test
can look green while proving nothing. Consult it *during* the loop, not after.

## The ticket is the spec

**A test is a spec claim in enforceable form.** The ticket's `## Spec` states what
must be true in prose; the test states the same thing so a machine can refuse it.
Prose that no test enforces is a wish. A test that enforces nothing the spec
claimed is scope you didn't agree to.

So the test names its ticket, in a comment beside it:

```rust
// CLI-68
#[test]
fn unnumbered_task_heading_exits_2() { … }
```

Where a spec makes several separately-testable claims, number them in the spec
and cite the claim: `// CLI-68/S2`. Where it makes one, the bare key is enough.
One claim may take several tests; one test discharges one claim — a test citing
three claims is usually three tests.

This is the one place an issue key belongs in source. The adapter's key policy
keeps keys out of production code, prose comments, and docs as decoration; a
citation is different in kind, because the key is the only stable name the spec
has. Keys stay resolvable after the board archives the issue, so the citation
does not rot.

### Which tickets owe a citation

Read the ticket's label; the answer follows from it.

| Label | Obligation |
|---|---|
| `feature` | At least one test citing this ticket, discharging a claim its spec makes |
| `bug` | The regression test cites this ticket. A bug is a claim the suite was missing or stating too weakly; a fix with no citing test is a fix nobody can prevent regressing |
| `refactor` | Change what no existing test asserts. The suite staying green *is* the proof — that is what makes it a refactor rather than a behavior change |
| `chore` | None |

An unlabeled ticket that changes observable behavior owes a citing test anyway;
the missing label is the defect, not the exemption.

### When the spec and the code disagree

Half of planning is discovering what the spec assumed. When implementation shows
a claim is wrong, unbuildable, or incomplete, **amend the spec and say so** —
`issue edit <KEY> --section spec` for the correction, `issue log` for why. Do not
quietly write the test to match what you built: that inverts the whole
discipline, leaving prose that describes an intention nobody honoured and a test
that can never disagree with the code.

## Red before green

**No implementation before a test that failed for the right reason.** A test
written after the code passes on first run, and a test that has never failed has
proven nothing about the code — only that the test can pass, which was never in
doubt.

Watch it fail, and read the failure:

- It **fails**, not errors. A compile error or a missing import is not red — fix
  it and re-run until the failure is the assertion.
- It fails **for the stated reason** — the behavior is absent, not the file, the
  fixture, or the spelling.
- If it **passes** on first run, the behavior already exists. Either the claim is
  already discharged (find the existing test and stop) or the test is aimed at
  the wrong thing (fix the test).

Wrote the implementation first? The test you write now is a description of what
you built, not of what was required. Delete the implementation and start from the
test. Keeping it "as a reference" is testing-after with extra steps — you will
adapt it, and the spec will end up matching the code instead of the code matching
the spec.

## Test seams

A **test seam** is the public boundary you observe behavior across without
reaching inside. Tests live at seams. Prefer an existing seam to a new one, and
prefer the highest seam that can still see the behavior — the fewer seams a
codebase has, the less of it is frozen by tests.

Name the seams in the plan's `**Test intent:**` field before writing a test, and
confirm them if the plan didn't already. A test at an unagreed seam is how a
suite ends up pinned to internals nobody meant to freeze.

> Not the same word as `scope-milestone`'s **slice boundary**, which is where one
> ticket ends and the next begins. That one partitions work; this one partitions
> observability.

## Anti-patterns

Four ways to hold a green suite that proves nothing.

**Implementation-coupled.** Mocks an internal collaborator, reaches a private
method, or verifies through a side channel — querying the database directly
instead of reading back through the interface. The tell: it breaks under a
refactor that changed no behavior. If a `refactor`-labeled ticket cannot stay
green without editing tests, those tests were coupled, and *that* is the finding
to log.

**Tautological.** The expected value is recomputed the way the code computes it,
so it agrees with the code by construction and can never disagree:

```
// tautological — expected is derived exactly as the implementation derives it
expect(total(items)).toBe(items.reduce((s, i) => s + i.price, 0))

// independent — the literal comes from the spec, a worked example, or arithmetic
// done by hand
expect(total([{price: 10}, {price: 5}])).toBe(15)
```

Expected values come from an independent source of truth. Under this discipline
that source is usually **the ticket's spec** — if the spec says exit 2, the test
asserts `2`, not `expected_exit_code()`.

**Horizontal slicing.** Writing every test for a task, then every implementation.
Bulk tests verify *imagined* behavior: they pin the shape you guessed at before
you learned anything, and they go insensitive to the changes that matter. Work
one claim at a time — one test, one implementation, then the next, each cycle
informed by what the last one taught. The plan's checkbox steps read
test-then-implement per *task*; inside a task, that still means one behavior at a
time, not a batch.

**Asserting the call, not the outcome.** `expect(payment.process).toHaveBeenCalled()`
proves the code called something, not that anything is true afterwards. Assert
the observable result.

## Mocking

Mock at **system boundaries only**: external APIs, time, randomness, and the
filesystem or database where a real one isn't practical (prefer a real test
database when it is). Never mock your own modules, internal collaborators, or
anything you control — that is the implementation-coupled anti-pattern with a
different name.

When a boundary is hard to substitute, that is a design finding, not a testing
problem: inject the dependency rather than constructing it inside, and prefer
several specific operations over one generic call taking a discriminator, so a
substitute returns one shape instead of branching on its arguments.
