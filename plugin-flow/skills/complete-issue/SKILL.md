---
name: complete-issue
description: "Take one cliban issue from backlog to finished: claim it, plan it against a fresh read of the code, execute the plan test-first, review at the gates, and move it on the board. Use when asked to work, start, pick up, implement, or finish a single ticket — and as the per-ticket body dispatched by complete-milestone."
requires_skills: [cliban-workflow]
---

# Complete Issue

One ticket, end to end. It already carries a `## Spec`; this skill supplies the `## Plan` and the code, and leaves the board telling the truth about both.

Two modes, differing only at the end:

- **Standalone** — you own the branch through to `in-review` or `done` and hand it to the user.
- **Dispatched by `complete-milestone`** — you are one agent in a wave. Commit on your ticket branch and **report**; the orchestrator merges. Never merge, never touch `main` or the milestone branch, never move the issue to `done`. Your brief says when you're in this mode.

## 1. Resolve and claim

The key the user named → `cliban issue current --json` → `cliban issue ls --ready --json` and ask. `--ready` is backlog + unblocked + unclaimed; a ticket that isn't ready is a stop-and-ask, not something to force.

```bash
cliban issue claim <KEY>
cliban issue mv <KEY> in-progress
```

**Dispatched as a subagent:** export `CLIBAN_ACTOR=agent:<KEY>` in every shell first. Subagents inherit the parent's session id, so without it every sibling claims as the same actor and claims stop excluding each other.

## 2. Read what the board knows

```bash
cliban issue show <KEY> --json
cliban issue cat <KEY> --section spec
cliban activity --issue <KEY>                     # approaches already tried and rejected
cliban project search <KEY-PROJ> "<terms>" --json
cliban milestone show "<name>" --project <KEY-PROJ> --json   # if the ticket has one
```

`activity --issue` is the one people skip and regret — on a reopened or handed-over ticket it's the difference between resuming and repeating.

The milestone description carries what the ticket's own spec cannot: **what was ruled out of scope**. Without it you will helpfully rebuild something the user explicitly declined.

## 3. Get on the right branch

Per the adapter's branch convention (`docs/agents/issue-tracker.md`). When dispatched, the worktree already exists — `cd` in and confirm isolation before writing anything.

## 4. Write the plan

Plan against a **fresh read of the code**, not the spec alone. The spec was written before anyone looked; half of planning is discovering what it assumed.

```bash
cliban issue edit <KEY> --section plan --create-section --description-file - <<'EOF'
### Task 1: <short name>
…
EOF
cliban issue lint <KEY>
cliban issue cat <KEY> --section plan     # confirm it actually landed
```

- **`--create-section` on the first write.** A ticket from `scope-milestone` has only `## Spec`, so a plain `--section plan` exits 2. Harmless once the section exists — pass it every time.
- **`lint` proves the plan parses, not that it exists.** On an issue with no `## Plan` it reports zero findings and exits 0, so a failed write and a clean plan look identical by exit code. Hence the `cat`.
- **Never write the whole description** — that destroys `## Spec` and `## Activity Log`.

Shape per the contract's *What a Good Plan Contains*, with `### Review Checkpoint:` markers between task groups. Each task's `**Test intent:**` names the test seams and which `## Spec` claims that task discharges — decide both now, while the spec is still being read rather than justified.

## 5. Execute

**Read [references/tdd.md](references/tdd.md) before the first test** — it governs this step: what a test cites, what a good test is, and the four ways a suite goes green while proving nothing.

Per behavior, one at a time:

1. **Red** — write the test, cite the ticket (`// <KEY>`), watch it fail *for the stated reason*. Not a compile error, not a missing fixture — the assertion.
2. **Green** — implement within the task's boundaries.
3. **Verify** — focused tests, then broader.
4. **Commit** — test and implementation together.
5. **Tick** — `cliban issue tick <KEY> --task N --step M` as each step lands, not when the task ends.

Batching every test for a task before any implementation is horizontal slicing: it pins the shape you guessed at before you learned anything. When implementation proves a spec claim wrong, amend `## Spec` and log why — never reshape the test to match what you built.

**Log the why**, not the what — `cliban issue log <KEY> "root cause was X"`, `"tried Y, fails because Z"`. cliban records what changed on its own; narration buries the signal.

**At every `### Review Checkpoint`, run the gate** — [references/review.md](references/review.md) governs it. One fresh-context reviewer over the group's cumulative diff, both verdicts in one dispatch. Record `HEAD` at each gate; it seeds the next gate's `BASE_SHA`, and the branch base seeds the first. Any spec ❌ or Critical/Important finding: fix, then re-review the same checkpoint. Only Minor: accept and continue. Log the outcome either way — a gate that left no trace is indistinguishable from one that never ran.

Review is batched to these markers deliberately; reviewing after every task is the tax that makes tickets crawl. A plan with no markers has one gate, at the end, which is a legitimate shape for a small ticket.

**Discovered scope gets promoted, never absorbed** — `cliban issue promote <KEY> --task N --step M --as sub-issue`, or a new issue with `--blocked-by <KEY>`. When a promoted child reaches `done`, tick the referencing step here; cliban doesn't mirror that for you.

## 6. The final gate

One cumulative review over the whole diff ([references/review.md](references/review.md), *The final gate*). It catches what a per-checkpoint view structurally cannot: two groups that each passed but disagree, drift from the spec, dead code, scope that grew a slice at a time.

Then build, typecheck, lint, and run the **full** suite. Per-task verification proves each task; only this proves the ticket. A failure here is unfinished work, not a finishing step.

## 7. Finish

Commit everything first, then:

**Standalone** — move the ticket to where the work actually is, then hand the branch over; merge/PR/discard is the user's call.

```bash
cliban issue mv <KEY> in-review --note "PR <url>"
cliban issue mv <KEY> done --note "merged as <sha>"
cliban linear push <KEY> || true      # only if the ticket came from Linear
```

**Dispatched** — report: commit SHA, branch, test status, one-line summary, **any amendment you made to `## Spec`**, and merge-risk notes. Report only after every commit has landed; never commit after reporting. The orchestrator moves it to `done`.

**Sweep one durable lesson**, both modes. What will still be true on the *next* ticket — a repo convention learned the hard way, non-obvious tool behavior, a hazard that will recur? Search first (`cliban project search`), then `project note add` or update the `###` that already covers it. It's an atomic append, so wave siblings sweeping concurrently is the intended shape, not a race. Most tickets teach nothing durable and sweep to zero; ticket-specific narration belongs in `issue log`.

**Stuck rather than finished?** Say which: `mv <KEY> blocked --note "<why>"` plus a linked blocker if it's outside this ticket, otherwise `issue release <KEY>` so someone else can take it.
