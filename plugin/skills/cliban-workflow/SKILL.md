---
name: cliban-workflow
description: "The cliban workflow contract: per-repo adapter binding, status mapping, where work artifacts live vs the repo, and how craft stacks (superpowers, mattpocock-skills, plan mode) publish to the board. Loaded by workflow skills via requires_skills. CLI mechanics live in the cliban skill, not here."
requires_skills: [cliban]
---

# Cliban Workflow — The Contract

This skill is policy, not mechanics. Every command, flag, JSON shape, trap, and the parseable-description grammar is specified once, in the `cliban` skill — consult it there; nothing below restates it. What lives here is the contract that workflow skills share: which repo binds to which board, what each workflow event means in board terms, and which artifacts belong on the board versus in git.

## Per-Repo Binding (the adapter)

A repo that has run `setup-cliban` carries `docs/agents/issue-tracker.md` — the adapter. It binds four things and is authoritative over the defaults below. Read it once per session before the first cliban action.

| Binding | Meaning | Default when no adapter |
|---|---|---|
| Project key | which board | basename of `git rev-parse --show-toplevel`, matched case-insensitively against `project ls` keys and names; ask on miss |
| Craft stack | who owns the craft of spec/plan/execute/finish | none — perform stages directly |
| Key policy | where issue keys may appear in git artifacts | branches and commit messages yes; source code, comments, docs never |
| Branch convention | what starting an issue does | switch to the issue's `git_branch_name` |

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

Move the ticket when the work moves, in the same breath — a board that lags reality is worse than no board. Linear-linked issues additionally get `cliban push linear KEY` after the `in-review` and `done` moves (linkage detection and field ownership: `cliban` skill, Linear bridge section).

## Where Artifacts Live

Work-lifecycle artifacts go on the board; knowledge that outlives the work goes in the repo. This supersedes any file-based storage a craft stack describes (superpowers' `docs/plans/`, mattpocock's `.scratch/`).

| Artifact | Home |
|---|---|
| Spec / PRD | issue `## Spec` |
| Implementation plan | issue `## Plan` |
| Progress, findings, dead ends | issue `## Activity Log` via `issue log` |
| Durable reusable lessons | project `## Notes`, search-first |
| Blocking relationships | relations via `--blocks`/`--blocked-by` — never `Blocked by:` text lines in repo files |
| ADRs, `CONTEXT.md`, domain docs | **the repo**, plaintext, git-tracked — never the board |

Issue keys follow the adapter's key policy; under every policy they stay out of source code, comments, and docs, because a key in code rots the moment the board archives the issue.

## What a Good Plan Contains

The `cliban` skill defines what *parses* (`### Task N:` headings, column-zero checkboxes). The contract for what a plan *says*, per task:

```markdown
### Task 1: short name

**Files:** exact paths

**Behaviors:** observable outcomes and edge cases

**Test intent:** what must fail before implementation and what the tests prove

- [ ] Add the failing behavior tests and verify the expected failure.
- [ ] Implement the behavior within the listed boundaries.
- [ ] Run focused and broader verification.
- [ ] Commit the coherent change.
```

Insert `### Review Checkpoint: <scope>` markers between task groups; executors pause there for a fresh-context review.

## Stage Mapping by Craft Stack

The bound stack owns the *craft* of each stage; this contract owns where its artifacts land.

- **superpowers** — `brainstorming` → `## Spec`; `writing-plans` → `## Plan`; `subagent-driven-development` executes with `tick`/`log`; `finishing-a-development-branch` drives the status moves.
- **mattpocock-skills** — reach a design any way (grilling, plan mode, conversation); `to-spec` publishes to `## Spec`; `to-tickets` creates issues with `--blocks` edges; `implement` reads the ticket, writes `## Plan`, drives TDD with `tick`/`log`; `triage` labels are ordinary cliban labels.
- **none** — plan mode or plain conversation for design; publish and execute directly: create the issue with its `## Spec`, round-trip the plan in via `issue edit --description-file -`, then `mv` → `tick` → `log` as the table above dictates.

## Shared Conventions

- **Labels:** prefer the canonical set `bug`, `feature`, `refactor`, `chore` (auto-created on first `--label` use; orphans are never garbage-collected).
- **Priority:** default `medium`; `high`/`urgent` only when explicitly indicated.
- **Scope discovery:** promote oversized steps (`issue promote`) or file a linked issue; never silently widen a ticket.
- **Promotion mirror:** when a promoted child reaches `done`, the skill that moved it also ticks the referencing step in the parent — cliban core deliberately does not auto-mirror.
