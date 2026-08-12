<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/readme/logo-dark.svg">
    <img src="assets/readme/logo.svg" width="330" alt="cliban">
  </picture>
</p>

<p align="center"><b>Durable memory and orchestration for coding agents.</b></p>

<p align="center">
  <a href="https://github.com/LioraLabs/cliban/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/LioraLabs/cliban/ci.yml?branch=main&style=flat-square&label=ci" alt="ci status"></a>
  <a href="https://github.com/LioraLabs/cliban/releases/latest"><img src="https://img.shields.io/github/v/release/LioraLabs/cliban?style=flat-square&label=release" alt="latest release"></a>
  <a href="https://crates.io/crates/cliban"><img src="https://img.shields.io/crates/v/cliban?style=flat-square&label=crates.io" alt="crates.io"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-lightgrey?style=flat-square" alt="MIT license"></a>
</p>

<p align="center"><img src="assets/board.png" width="880" alt="the cliban board"></p>

Agents lose context. Teams of agents also collide, duplicate work, and leave
half-finished branches behind. cliban gives them a durable local control plane:
specs, plans, progress, dependencies, claims, decisions, and reusable lessons
live on one SQLite board and survive crashes, compaction, and handoffs.

```console
$ cliban issue current --json                 # recover the ticket for this branch
$ cliban issue cat PROJ-42 --section plan     # recover the living plan
$ cliban activity --issue PROJ-42             # recover decisions and dead ends
$ cliban project search PROJ "wal mode" --json # recall durable project knowledge
```

Every read is available as JSON. Every mutation is atomic. Every event is
attributed. Nothing requires a hosted service.

<p align="center"><img src="assets/tour.gif" width="880" alt="a lap around the board: scope a project, move a card, check the milestone, read the timeline"></p>
<p align="center"><sub>scope · coordinate · observe · recover</sub></p>

## The agent system

The board is the memory. The shipped skills are the operating system around it:

| Skill | What it does |
|---|---|
| `cliban` | Teaches agents the complete, machine-readable CLI surface. |
| `setup-cliban` | Binds a repository to its board and injects live state into new sessions. |
| `cliban-workflow` | Defines the shared artifact, status, git, and recovery contract. |
| `explore-feature` | Turns a rough idea into an approved design. |
| `scope-milestone` | Grills the design, slices tracer-bullet tickets, and records dependency edges. |
| `triage-bug` | Searches duplicates, reproduces the report, and leaves an actionable ticket. |
| `diagnose-issue` | Proves root cause while preserving the hypothesis ledger on the ticket. |
| `complete-issue` | Plans proportionally, implements, verifies, and hands off one ticket. |
| `complete-milestone` | Dispatches one agent per ticket in dependency waves and isolated worktrees. |
| `recover-milestone` | Reconstructs an interrupted run from board and git state. |

`cliban-flow` now owns the lifecycle through a small dispatcher: ticket start,
sync, ready, integration, abandonment, and milestone recovery use the same
board-and-git contract. Plans may be one sentence or a structured checklist;
verification is executable; review is spent where risk justifies it. End-to-end
scenarios exercise both standalone tickets and orchestrated milestone tickets.

```text
idea → explore → scope → dependency waves → isolated agents → verified handoff
                  bug → triage → diagnose → complete ────────┘
                                      ↑
                          board + git make it resumable
```

Install the plugins:

```bash
claude plugin marketplace add LioraLabs/claude-plugins
claude plugin install cliban@lioralabs
claude plugin install cliban-flow@lioralabs
```

`cliban` contains the CLI skill and repository binding. `cliban-flow` adds the
workflow skills; it is optional and depends on `cliban` for mechanics.

<p align="center"><img src="assets/agent.gif" width="880" alt="the agent loop: read the plan, tick a step, log a finding, read the attributed timeline"></p>
<p align="center"><sub>read · act · tick · log · resume</sub></p>

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/LioraLabs/cliban/main/install.sh | sh
```

Or:

```sh
brew install lioralabs/tap/cliban
cargo binstall cliban
cargo install cliban
```

```bash
cliban project add PROJ "My project"
cliban issue add "First issue" --project PROJ --priority high
cliban
```

## Memory that agents can mutate safely

An issue description is structured Markdown. Agents replace one section without
touching the others:

```markdown
## Spec
## Plan
## Activity Log
## Notes
```

```bash
cliban issue edit PROJ-42 --section spec --description-file -
cliban issue tick PROJ-42 --task 1 --step 2
cliban issue log PROJ-42 "reproduction fails after the second reorder"
cliban issue lint PROJ-42
```

Project notes are long-lived knowledge, retrieved progressively instead of
dumped into every prompt:

```bash
cliban project note add PROJ "tests share a tempdir" --body -
cliban project search PROJ "flaky tempdir" --json
```

Claims prevent duplicate work. Native blocking edges produce dispatchable waves:

```bash
cliban issue ls --ready --project PROJ --json
cliban issue claim PROJ-42
cliban milestone waves --project PROJ "v1" --json
```

## Human control plane

Run `cliban` with no arguments for the TUI: five status columns, fuzzy search,
milestone progress, projects, and an attributed activity feed. Press `e` to edit
an issue as frontmatter plus Markdown in `$EDITOR`.

<p align="center"><img src="assets/edit.gif" width="880" alt="press e, edit the issue in vim, save, and the card moves columns"></p>
<p align="center"><sub>`e` → edit → save → synchronized board</sub></p>

## Connect when needed

- **Linear:** import assigned work, keep local plans and logs, then push one
  living progress comment. Field ownership prevents re-imports from eating an
  agent's plan.
- **Other harnesses:** [`plugin/skills/cliban/SKILL.md`](plugin/skills/cliban/SKILL.md)
  follows the Agent Skills format and is checked against the real CLI in CI.

## Why a board instead of a plan file?

A file remembers text. cliban remembers work: ownership, dependencies, atomic
plan progress, searchable lessons, and an attributed timeline. A fresh agent can
query what is ready, what changed, what failed, and where to resume without
reconstructing the answer from prose and git diffs.

Everything stays local unless you explicitly connect Linear.

## Build

```bash
cargo test --workspace
```

cliban is pre-1.0 and MIT licensed.
