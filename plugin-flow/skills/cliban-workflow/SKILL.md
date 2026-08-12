---
name: cliban-workflow
description: "The cliban workflow contract: per-repo adapter binding, status mapping, where work artifacts live vs the repo, and what a good plan contains. Loaded by workflow skills via requires_skills. CLI mechanics live in the cliban skill, not here."
requires_skills: [cliban]
---

# Cliban Workflow — The Contract

Policy, not mechanics. Every command, flag, JSON shape, and the description grammar lives in the **`cliban:cliban`** skill, which ships in the separate `cliban` plugin this one depends on.

**Invoke `cliban:cliban` now** if you have not already — it does not load on its own, and nothing below restates it. If it is not among your available skills, say so rather than guessing at the CLI.

## Per-Repo Binding (the adapter)

A repo that has run `setup-cliban` carries `docs/agents/issue-tracker.md` — the adapter. It records the project, dispatcher workspace, reviewer, and key-placement invariant. Read it once per session before the first cliban action.

| Binding | Meaning | Default when no adapter |
|---|---|---|
| Project key | which board | basename of `git rev-parse --show-toplevel`, matched case-insensitively against `project ls` keys and names; ask on miss |
| Key placement | where issue keys appear in git artifacts | dispatcher branches and commits carry them; source code, comments, tests, and docs never do |
| Workspace convention | where dispatcher-started work lives | isolated `.worktrees/<git_branch_name>` worktree |
| Reviewer | who runs the ticket's once-by-default review and exceptional checkpoints | a general-purpose agent with `complete-issue`'s inline brief |

If the user is wiring up a new repo, point them at `setup-cliban`; don't improvise a binding. If `cliban` itself is missing from `$PATH`, skip all board actions silently for the session.

## The Dispatcher

The workflow's git-and-board transitions live at `../../scripts/cliban-flow` relative to this `SKILL.md` (`${CLAUDE_PLUGIN_ROOT}/scripts/cliban-flow` under Claude). The workflow skills abbreviate that executable as `cliban-flow`; resolve that one path before using them. If it is absent or not executable, stop and say so. There is no fallback: enforcing this protocol only when convenient is the failure the dispatcher removes.

Its surface is `milestone start`, `milestone status`, `milestone finish`, `milestone abandon`, `ticket start`, `ticket status`, `ticket sync`, `ticket ready`, `ticket integrate`, and `ticket abandon`. Invoke the subcommand instead of describing or recreating the git operation it owns. Exit 0 is success or an affirmative verdict, exit 1 is a legitimate negative verdict with its next step, and exit 2 is a refusal whose instruction must be followed before retrying.

## Status Mapping

| Workflow event | Board action |
|---|---|
| Ticket work starts (standalone or dispatched) | `ticket start KEY` claims it, creates its workspace, and moves it to `in-progress` |
| Stuck on a dependency | `mv KEY blocked --note "<why>"` |
| Ticket ready (standalone or dispatched) | `ticket ready KEY` records its immutable HEAD and moves it to `in-review` |
| PR merged / local merge | `mv KEY done --note "merged as <sha>"` |
| Ticket abandoned with human confirmation | `ticket abandon KEY --confirm "<why>"`; keep status, log why, release claim |
| Milestone abandoned with human confirmation | `milestone abandon NAME -p PROJECT --confirm "<why>"`; apply the ticket rule to every issue |

Move the ticket when the work moves, in the same breath — a board that lags reality is worse than no board. Linear bridge synchronization remains a separate explicit action after lifecycle transitions (linkage detection and field ownership: `cliban` skill, Linear bridge section).

## Where Artifacts Live

Work-lifecycle artifacts go on the board; knowledge that outlives the work goes in the repo. This supersedes any file-based storage another skill describes — a plan or spec belongs in its issue, not in a `docs/plans/` or `.scratch/` file.

| Artifact | Home |
|---|---|
| Spec / PRD | issue `## Spec` via `issue edit KEY --section spec` |
| Implementation plan | issue `## Plan` via `issue edit KEY --section plan`; freeform is valid, while `lint`/`tick`/`promote` are available for structured plans |
| Progress, findings, dead ends | issue `## Activity Log` via `issue log` |
| Durable reusable lessons | project `## Notes` via `project note add`, search-first |
| Blocking relationships | relations via `--blocks`/`--blocked-by` — never `Blocked by:` text lines in repo files |
| ADRs, `CONTEXT.md`, domain docs | **the repo**, plaintext, git-tracked — never the board |

The key-placement invariant keeps issue keys in dispatcher branches and commits,
and out of source code, comments, tests, and docs. Provenance lives in git and on
the board.

## What a Good Plan Contains

A plan is proportional to the work. A sentence or short approach is enough for
small work; larger work may use ordered `### Task N:` headings and column-zero
checkboxes. When that structure is present, `issue lint`, `issue tick`, and
`issue promote` can validate and mutate it. They are tools, not lifecycle gates.

Name observable outcomes, important edge cases, and the executable evidence
that will prove meaningful claims. Add a mid-ticket review checkpoint only where
a mistaken foundation would compound expensively. Non-trivial or risky work gets
one fresh-context review over the complete ticket diff once by default before
ready; exceptional checkpoints supplement rather than replace it.

## The Stages

Two ways onto the board — building something, or something being broken — converging on the same executors. Each stage owns one artifact from the table above:

| Stage | Skill | Lands |
|---|---|---|
| Design | `explore-feature` | a ticket, or an empty milestone, carrying `## Spec` |
| Slice | `scope-milestone` | tracer-bullet issues with `--blocked-by` edges |
| Report → ticket | `triage-bug` | a `bug` issue whose `## Spec` holds a reproduction |
| Root cause | `diagnose-issue` | the hypothesis ledger and proven cause in `## Activity Log` |
| Execute one | `complete-issue` | proportional `## Plan`, implementation, executable verification, and dispatcher-owned start/ready |
| Execute many | `complete-milestone` | wave-ordered tickets merged onto a milestone branch |
| Recover | `recover-milestone` | read-only diagnosis from the board and git |

```
idea    → explore-feature → scope-milestone ─┐
                                             ├→ complete-issue → (complete-milestone)
report  → triage-bug      → diagnose-issue ──┘
```

Working without them — plan mode, or plain conversation — is fully supported and
changes nothing about the contract: create the issue with its `## Spec`, start it
through the dispatcher, write and confirm a proportional plan without replacing
the whole description, prove the result with executable evidence, then hand its
committed HEAD back through dispatcher ready.
