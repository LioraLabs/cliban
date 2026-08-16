---
name: diagnose-issue
description: "Find and prove the root cause of a bug on a cliban ticket, logging the hypothesis ledger as you go. Use to diagnose, debug, or investigate something broken, throwing, failing, or slow that already has a ticket. Ends at a proven cause; complete-issue writes the fix."
requires_skills: [cliban-workflow]
---

# Diagnose Issue

Find the cause and **prove** it. This skill does not fix — it ends when the
cause is demonstrated and logged; `complete-issue` turns the reproduction into
a regression test and the fix.

**Load first:** `cliban-flow:cliban-workflow` and `cliban:cliban` — neither
loads on its own.

**The iron law:** no fix proposals before a demonstrated root cause. **The
ticket is the ledger:** every hypothesis, result, and dead end goes to
`issue log` as it lands, not summarised at the end — context dies, the board
doesn't.

## 1. Read what's known

```bash
cliban issue cat <KEY> --section spec        # symptom, reproduction, environment
cliban activity --issue <KEY>                # theories already tried and killed
cliban issue mv <KEY> in-progress
```

`activity --issue` first, always — re-running a killed hypothesis is how a
second session wastes a day. No reproduction in the spec and none obvious?
That's `triage-bug`'s half; do it properly before diagnosing a symptom you
can't trigger.

## 2. Build a feedback loop — this is the skill

Everything after is mechanical. With a tight loop that goes red on this bug,
bisection, instrumentation, and hypothesis-testing all just consume it;
without one, reading code won't save you. Spend disproportionate effort here.
In order of preference:

1. A **failing test** at whatever seam reaches the bug.
2. A **CLI invocation or curl** against a fixture, diffed against known-good.
3. A **replayed trace** — a captured request or event log run through the path
   in isolation.
4. A **throwaway harness** — the minimum subset reaching the code path in one
   call.
5. A **property or fuzz loop**, when the symptom is "sometimes wrong".
6. A **bisect harness**, when a known-good commit exists — automate the check
   so `git bisect run` drives it.
7. A **differential loop** — one input through two versions or configs, diff.

Then tighten it: faster, sharper (assert the exact symptom, not "didn't
crash"), deterministic (pin time, seed RNG, isolate the filesystem).
Non-deterministic bugs: raise the reproduction rate — loop the trigger 100×,
parallelise, add stress, narrow the timing window; 50% is debuggable, 1% is
not.

Done when one already-run command drives the real code path, asserts the
user's exact symptom, gives the same verdict every run, in seconds,
unattended. Log it:

```bash
cliban issue log <KEY> "Loop: \`cargo test -p cliban ordering::collapses_after_50\` — red in 1.4s, deterministic"
```

Cannot build one? Stop and say so: list what you tried, ask for what would
unblock it (environment access, a redacted HAR/log/core dump, permission to
instrument), `mv <KEY> blocked --note`, and log the ask. Never hypothesise
without a loop.

## 3. Reproduce and minimise

Watch it go red; confirm it's the failure the user described, not a neighbour.
Shrink to the smallest still-red scenario, cutting one element at a time,
until every remaining element is load-bearing. The minimised repro collapses
the hypothesis space and becomes the regression test.

## 4. Hypothesise — three to five, ranked, before testing any

One hypothesis anchors you to the first plausible story. Each must be
falsifiable — "if X is the cause, changing Y makes it disappear"; no
prediction means no hypothesis. Log the ranked list and show the user — they
often re-rank it instantly — but proceed on your own ranking if they're away.

```bash
cliban issue log <KEY> "H1 (likely): … — predict: … H2: … H3: …"
```

## 5. Test, one variable at a time

Each probe maps to a prediction; change one thing per run. A debugger or REPL
beats logs where the environment allows; otherwise targeted logs at the
boundaries that distinguish hypotheses. Tag every probe with a unique prefix
(`[DEBUG-a4f2]`) and log the tag — an untagged probe is the one that ships.
Multi-component systems: instrument each boundary once to find which layer
fails before opening any layer up. Performance: measure a baseline, then
bisect — logs are the wrong tool.

Log each result as it lands, killed hypotheses included. Three hypotheses dead
and the symptom still moving is an architectural finding, not a hypothesis
failure — log it as such and raise it with the user before continuing.

## 6. Prove it, then hand off

Proven means both directions: the symptom disappears when you change the
suspected cause and comes back when you revert it. Then:

```bash
grep -rn '\[DEBUG-a4f2\]' .            # every probe removed
cliban issue log <KEY> "Root cause: <what, where, why>. Proven by <the flip test>."
```

Add the cause to the spec (`issue edit <KEY> --section spec`), remove the
instrumentation, delete throwaway harnesses (or say where one lives and why it
earned its keep), and keep the minimised repro. Report the cause, the loop
command, and the repro, then hand to `complete-issue` — either
continue directly into `complete-issue` in the same session keeping the
claim, or run
`cliban issue release <KEY>` before reporting the handoff. No correct seam for
a regression test is itself a finding — log it and say so; that architecture
gap is worth its own ticket.
