---
name: diagnose-issue
description: "Find the root cause of a bug on a cliban ticket and prove it, logging the hypothesis ledger to the ticket as you go. Use when asked to diagnose, debug, or investigate something broken, throwing, failing, or slow that already has a ticket. Ends at a proven cause; complete-issue writes the fix."
requires_skills: [cliban-workflow]
---

# Diagnose Issue

Find the cause and **prove** it. This skill does not fix — it ends when the cause is demonstrated and written down, and `complete-issue` turns the reproduction into a regression test and the fix.

**Load first:** invoke `cliban-flow:cliban-workflow` for the contract (status mapping, where each artifact lands) and `cliban:cliban` for CLI mechanics. Neither loads on its own — reach for them with the Skill tool before the first board action.

**The iron law:** no fix proposals before a root cause you have demonstrated. A symptom fix that happens to work is indistinguishable from one that hides the bug until later.

**The ticket is the ledger.** Every hypothesis, result, and dead end goes to `## Activity Log` as you go — not summarised at the end. Context dies; the board doesn't. `cliban activity --issue <KEY>` is what makes a diagnosis resumable by a fresh session, and it is the first thing the next person reads.

## 1. Read what's already known

```bash
cliban issue show <KEY> --json
cliban issue cat <KEY> --section spec        # symptom, reproduction, environment
cliban activity --issue <KEY>                # hypotheses already tried and killed
cliban issue mv <KEY> in-progress
```

`activity --issue` first, always. On a reopened or handed-over bug it names the theories already ruled out — re-running one is the most common way a second session wastes a day.

No reproduction in the spec, and none obvious? That is `triage-bug`'s job; go back and do that half properly rather than diagnosing a symptom you can't trigger.

## 2. Build a feedback loop — this is the skill

Everything after this is mechanical. With a **tight** loop that goes red on *this* bug you will find the cause; bisection, instrumentation, and hypothesis-testing all just consume it. Without one, no amount of reading code will save you.

Spend disproportionate effort here. Be aggressive, be creative, refuse to give up. Roughly in order of preference:

1. A **failing test** at whatever seam reaches the bug.
2. A **CLI invocation or curl** against a fixture, diffed against known-good output.
3. A **replayed trace** — a captured request, payload, or event log run through the path in isolation.
4. A **throwaway harness** — the minimum subset of the system that reaches the code path in one call.
5. A **property or fuzz loop**, when the symptom is "sometimes wrong".
6. A **bisect harness**, when it worked at a known-good commit: automate "boot at X, check, repeat" so `git bisect run` can drive it.
7. A **differential loop** — same input through two versions or two configs, diff the outputs.

Then **tighten** it: faster (cache setup, narrow scope), sharper (assert the exact symptom, not "didn't crash"), more deterministic (pin time, seed RNG, isolate the filesystem). A 30-second flaky loop is barely better than none; a 2-second deterministic one is a superpower.

**Non-deterministic bugs:** the goal is a higher reproduction rate, not a clean repro. Loop the trigger 100×, parallelise, add stress, narrow the timing window. 50% is debuggable; 1% is not — keep raising it.

**Done when** you can name one command you have already run, that drives the real code path, asserts the user's exact symptom, gives the same verdict every run, takes seconds, and needs no human. Log it:

```bash
cliban issue log <KEY> "Loop: \`cargo test -p cliban ordering::collapses_after_50\` — red in 1.4s, deterministic"
```

**Cannot build one?** Stop and say so. List what you tried, and ask for what would unblock it: environment access, a redacted artifact (HAR, log dump, core dump), or permission to instrument production. `mv <KEY> blocked --note` and log the ask. Do **not** proceed to hypothesise without a loop — jumping to a theory is the exact failure this skill exists to prevent.

## 3. Reproduce and minimise

Watch it go red. Confirm it's the failure the *user* described, not a different one nearby — wrong bug, wrong fix.

Then shrink to the smallest scenario still red. Cut inputs, callers, config, and steps **one at a time**, re-running after each cut. Done when every remaining element is load-bearing: remove any one and it goes green.

Minimising is not busywork — it collapses the hypothesis space in the next step, and it becomes the regression test `complete-issue` writes.

## 4. Hypothesise — three to five, ranked, before testing any

Generating one hypothesis anchors you to the first plausible story. Generate several and rank them.

Each must be **falsifiable** — state the prediction: *"If X is the cause, then changing Y makes it disappear."* Can't state a prediction? It's a vibe. Sharpen or discard it.

Log the ranked list, then show it to the user. They often re-rank it instantly ("we deployed a change to #3 yesterday") or know one is already dead. Cheap checkpoint, large payoff — but don't block on it; proceed with your own ranking if they're away.

```bash
cliban issue log <KEY> "H1 (likely): f64 positions lose precision past ~50 midpoint inserts — predict: seeding integer positions makes it vanish. H2: … H3: …"
```

## 5. Instrument and test, one variable at a time

Each probe maps to a specific prediction. Change one thing per run.

- **A debugger or REPL beats logs** where the environment allows it. One breakpoint beats ten prints.
- **Targeted logs at the boundaries that distinguish hypotheses** — never "log everything and grep".
- **Tag every probe** with a unique prefix — `[DEBUG-a4f2]` — so cleanup is one grep. Log the tag on the ticket; an untagged probe is the one that ships.
- **Multi-component systems:** instrument each boundary once — what enters, what exits, whether config propagated — to find *which* layer fails before investigating any layer's internals.
- **Performance:** logs are usually the wrong tool. Establish a baseline (timing harness, profiler, query plan), then bisect. Measure first.

Log each result as it lands, killed hypotheses included. A dead end recorded is worth as much as the hit — it's what stops the next session repeating it.

**Three hypotheses dead and the symptom keeps moving?** Stop generating a fourth. That pattern — each fix or theory revealing a new problem somewhere else — is an architectural finding, not a hypothesis failure. Log it as such and raise it with the user before continuing.

## 6. Prove it, then hand off

A cause is proven when you can do both: **make the symptom disappear** by changing the suspected cause, and **bring it back** by reverting that change. One direction alone is correlation.

Then, before you finish:

```bash
grep -rn '\[DEBUG-a4f2\]' .            # every probe removed
cliban issue edit <KEY> --section spec --description-file -   # symptom stands; add the cause
cliban issue log <KEY> "Root cause: <what, where, why it produces this symptom>. Proven by <the flip test>."
```

Remove your instrumentation and delete throwaway harnesses — or, if a harness earned its keep, say where it lives and why. Leave the minimised reproduction: it's the regression test.

Report the root cause, the loop command, and the minimised repro, then hand to `complete-issue` — it writes the failing test at the right seam, the fix, and the status move.

**If there is no correct seam for a regression test, that is itself a finding.** A test that can't exercise the real bug pattern as it occurs at the call site gives false confidence. Log it and say so in the handoff; the architecture is preventing the bug from being locked down, and that is worth its own ticket.
