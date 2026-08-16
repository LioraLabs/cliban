---
name: cliban-workflow
description: "The cliban workflow contract: adapter binding, dispatcher surface, status mapping, artifact homes, plan policy. Loaded by the flow skills via requires_skills; CLI mechanics live in the cliban skill."
requires_skills: [cliban]
---

# Cliban Workflow — The Contract

Policy, not mechanics. Commands, flags, and JSON shapes live in the
**`cliban:cliban`** skill — invoke it now if you have not; it does not load on
its own. If it is missing from your skills, say so rather than guessing at the
CLI.

## The adapter

A repo wired by `setup-cliban` carries `docs/agents/issue-tracker.md`. Read it
once per session before the first board action. It binds:

| Binding | Default without an adapter |
|---|---|
| Project key | repo basename matched case-insensitively against `project ls`; ask on miss |
| Key placement | issue keys live in dispatcher branches and commits — never in source, comments, tests, or docs |
| Workspace | isolated `.worktrees/<git_branch_name>` worktree |
| Reviewer | general-purpose agent with `complete-issue`'s review brief |

Wiring a new repo is `setup-cliban`'s job; don't improvise a binding. If
`cliban` is missing from `$PATH`, skip all board actions silently this session.

## The dispatcher

Git-and-board transitions live in one executable: `../../scripts/cliban-flow`
relative to this file (`${CLAUDE_PLUGIN_ROOT}/scripts/cliban-flow` under
Claude), abbreviated `cliban-flow`. Resolve that path before use; if it is
absent or not executable, stop and say so — there is no fallback.

Surface: `milestone start`, `milestone status`, `milestone finish`,
`milestone abandon`, `ticket start`, `ticket status`, `ticket sync`,
`ticket ready`, `ticket integrate`, `ticket abandon`. Invoke the subcommand
instead of recreating the git operation it owns. Exit 0 = success or an
affirmative verdict; 1 = a legitimate negative verdict with its next step;
2 = a refusal whose instruction must be followed before retrying.

## Status mapping

| Workflow event | Board action |
|---|---|
| Work starts (standalone or dispatched) | `ticket start KEY` — claims, creates the workspace, moves to `in-progress` |
| Stuck on a dependency | `mv KEY blocked --note "<why>"` |
| Ticket ready (standalone or dispatched) | `ticket ready KEY` — records its immutable HEAD, moves to `in-review` |
| Merged | `mv KEY done --note "merged as <sha>"` |
| Ticket abandoned (human-confirmed) | `ticket abandon KEY --confirm "<why>"` |
| Milestone abandoned (human-confirmed) | `milestone abandon NAME -p PROJECT --confirm "<why>"` |

Move the ticket when the work moves, in the same breath. Linear bridge sync
stays a separate explicit action after lifecycle moves (`cliban` skill, Linear
bridge section).

## Where artifacts live

Work-lifecycle artifacts go on the board; knowledge that outlives the work
goes in the repo. This supersedes any file-based storage another skill
describes — a plan or spec belongs in its issue, not in a `docs/plans/` file.

| Artifact | Home |
|---|---|
| Spec / PRD | issue `## Spec` (`issue edit KEY --section spec`) |
| Implementation plan | issue `## Plan` (`--section plan`) |
| Progress, findings, dead ends | `issue log` |
| Durable reusable lessons | project `## Notes` (`project note add`, search-first) |
| Blocking relationships | relations (`--blocks`/`--blocked-by`), never prose |
| ADRs, domain docs | the repo, git-tracked — never the board |

## Plans

Proportional to the work: a sentence for small work; ordered `### Task N:`
headings with column-zero checkboxes for large. Freeform is valid;
`lint`/`tick`/`promote` are tools, not lifecycle gates. Name observable
outcomes, edge cases, and the executable evidence that will prove each claim.
Non-trivial or risky work gets one fresh-context review over the complete
ticket diff once by default before ready; add a mid-ticket checkpoint only
where a wrong foundation compounds expensively.

## The stages

| Stage | Skill | Lands |
|---|---|---|
| Design | `explore-feature` | a container carrying `## Spec` |
| Slice | `scope-milestone` | tracer bullets with blocking edges |
| Report → ticket | `triage-bug` | a `bug` issue with a reproduction |
| Root cause | `diagnose-issue` | the proven cause in the activity log |
| Execute one | `complete-issue` | plan, work, proof, dispatcher start/ready |
| Execute many | `complete-milestone` | wave-ordered tickets on a milestone branch |
| Recover | `recover-milestone` | read-only diagnosis |

```
idea    → explore-feature → scope-milestone ─┐
                                             ├→ complete-issue → (complete-milestone)
report  → triage-bug      → diagnose-issue ──┘
```

Working without them — plan mode, plain conversation — changes nothing about
the contract: create the issue with its `## Spec`, `ticket start KEY`, write
and confirm a proportional plan without replacing the whole description, prove
the result with executable evidence, then `ticket ready KEY`.
