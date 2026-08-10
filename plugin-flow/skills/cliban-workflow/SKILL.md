---
name: cliban-workflow
description: "The cliban workflow contract: per-repo adapter binding, status mapping, where work artifacts live vs the repo, and what a good plan contains. Loaded by workflow skills via requires_skills. CLI mechanics live in the cliban skill, not here."
requires_skills: [cliban]
---

# Cliban Workflow — The Contract

Policy, not mechanics. Every command, flag, JSON shape, and the description grammar lives in the **`cliban:cliban`** skill, which ships in the separate `cliban` plugin this one depends on.

**Invoke `cliban:cliban` now** if you have not already — it does not load on its own, and nothing below restates it. If it is not among your available skills, say so rather than guessing at the CLI.

## Per-Repo Binding (the adapter)

A repo that has run `setup-cliban` carries `docs/agents/issue-tracker.md` — the adapter. It binds four things and is authoritative over the defaults below. Read it once per session before the first cliban action.

| Binding | Meaning | Default when no adapter |
|---|---|---|
| Project key | which board | basename of `git rev-parse --show-toplevel`, matched case-insensitively against `project ls` keys and names; ask on miss |
| Key policy | where issue keys may appear in git artifacts | branches and commit messages yes; source code, comments, docs never — except a test citing its ticket |
| Branch convention | what starting an issue does | switch to the issue's `git_branch_name` |
| Reviewer | who runs the gate at a plan's review checkpoints | a general-purpose agent with `complete-issue`'s inline brief |

If the user is wiring up a new repo, point them at `setup-cliban`; don't improvise a binding. If `cliban` itself is missing from `$PATH`, skip all board actions silently for the session.

## Status Mapping

| Workflow event | Board action |
|---|---|
| Spec/plan written | issue stays `backlog` |
| First step picked up | `mv KEY in-progress` |
| Stuck on a dependency | `mv KEY blocked --note "<why>"` |
| PR opened | `mv KEY in-review --note "PR <url>"` |
| PR merged / local merge | `mv KEY done --note "merged as <sha>"` |
| Discarded / abandoned | keep status, `issue log KEY "work discarded: <why>"` |

Move the ticket when the work moves, in the same breath — a board that lags reality is worse than no board. Linear-linked issues additionally get `cliban linear push KEY` after the `in-review` and `done` moves (linkage detection and field ownership: `cliban` skill, Linear bridge section).

## Where Artifacts Live

Work-lifecycle artifacts go on the board; knowledge that outlives the work goes in the repo. This supersedes any file-based storage another skill describes — a plan or spec belongs in its issue, not in a `docs/plans/` or `.scratch/` file.

| Artifact | Home |
|---|---|
| Spec / PRD | issue `## Spec` via `issue edit KEY --section spec` |
| Implementation plan | issue `## Plan` via `issue edit KEY --section plan`, then `issue lint KEY` |
| Progress, findings, dead ends | issue `## Activity Log` via `issue log` |
| Durable reusable lessons | project `## Notes` via `project note add`, search-first |
| Blocking relationships | relations via `--blocks`/`--blocked-by` — never `Blocked by:` text lines in repo files |
| ADRs, `CONTEXT.md`, domain docs | **the repo**, plaintext, git-tracked — never the board |

Issue keys follow the adapter's key policy, which keeps them out of source code, comments, and docs as decoration. **One exception under every policy:** a test may cite the ticket whose `## Spec` it discharges, in a comment beside it. That is a citation, not decoration — the test *is* that spec in enforceable form, the key is the only stable name the spec has, and keys stay resolvable after archive. Shape and obligations: `complete-issue`'s `references/tdd.md`.

## What a Good Plan Contains

The `cliban` skill defines what *parses* (`### Task N:` headings, column-zero checkboxes). The contract for what a plan *says*, per task:

```markdown
### Task 1: short name

**Files:** exact paths

**Behaviors:** observable outcomes and edge cases

**Test intent:** the seams the tests observe from, and which claims of the ticket's `## Spec` each test discharges

- [ ] For each behavior in turn: failing test citing the ticket, verified failure, then the implementation.
- [ ] Run focused and broader verification.
- [ ] Commit the coherent change.
```

Insert `### Review Checkpoint: <scope>` markers between task groups. Each is a **gate**: the executor stops, reviews every task since the previous marker in one pass, and does not advance with a spec failure or a serious quality issue open.

Place them where a bug would otherwise **compound** — after a foundational slice later tasks stack on, or where the work crosses subsystems. Not after every task; batching is the point, since N tasks cost one review instead of N. A plan with no markers has one gate at the end, which is the right shape for a small ticket — a decision, not an omission.

The first step is a loop, not two steps: every test then every implementation is horizontal slicing, pinning the shape you guessed at before the first line taught you anything.

## The Stages

Two ways onto the board — building something, or something being broken — converging on the same executors. Each stage owns one artifact from the table above:

| Stage | Skill | Lands |
|---|---|---|
| Design | `explore-feature` | a ticket, or an empty milestone, carrying `## Spec` |
| Slice | `scope-milestone` | tracer-bullet issues with `--blocked-by` edges |
| Report → ticket | `triage-bug` | a `bug` issue whose `## Spec` holds a reproduction |
| Root cause | `diagnose-issue` | the hypothesis ledger and proven cause in `## Activity Log` |
| Execute one | `complete-issue` | `## Plan`, then code, `tick`, `log`, and a status move |
| Execute many | `complete-milestone` | wave-ordered tickets merged onto a milestone branch |

```
idea    → explore-feature → scope-milestone ─┐
                                             ├→ complete-issue → (complete-milestone)
report  → triage-bug      → diagnose-issue ──┘
```

Working without them — plan mode, or plain conversation — is fully supported and changes nothing about the contract: create the issue with its `## Spec`, write the plan via `issue edit KEY --section plan --create-section --description-file -` (never a whole-description rewrite), `issue lint KEY` to confirm it parses, then `mv` → `tick` → `log` as the status table dictates.

## Shared Conventions

- **Labels:** prefer the canonical set `bug`, `feature`, `refactor`, `chore` (auto-created on first `--label` use; orphans are never garbage-collected).
- **Priority:** `medium` by default, passed explicitly — the CLI's own default is `none`. `high`/`urgent` only when indicated.
- **Scope discovery:** promote oversized steps (`issue promote`) or file a linked issue; never silently widen a ticket.
- **Take work via the frontier:** `issue ls --ready` answers "what can I start"; `issue claim` before starting anything another session might also see (attribution is automatic per session).
- **Racy edits:** any read-modify-write of a description carries `--if-updated-at` from the read — but prefer the atomic tools (`--section`, `append-section`, `log`, `tick`, `note add`), which need no round-trip at all.
- **Custom sections:** the four contract H2s are reserved, but any other H2 is fair game and addressable by verbatim anchor (`--section "Decisions so far"`).
- **Promotion mirror:** when a promoted child reaches `done`, the skill that moved it also ticks the referencing step in the parent — cliban core deliberately does not auto-mirror.
