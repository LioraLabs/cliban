---
name: setup-cliban
description: "Bind a repo to its cliban board — project key, fixed key placement and dispatcher workspace, and reviewer. Run once per repo; re-run to change the binding."
disable-model-invocation: true
requires_skills: [cliban]
---

# Setup Cliban

Scaffold the per-repo binding that every cliban skill reads:

- **The adapter** — `docs/agents/issue-tracker.md`, declaring cliban as this repo's issue tracker and recording the project key, dispatcher workspace, reviewer, and key-placement invariant
- **The pointer** — an `## Agent skills` block in `CLAUDE.md` / `AGENTS.md` so agents find the adapter
- **The board** — the cliban project itself, created if missing

This is a prompt-driven skill, not a deterministic script. Explore, present what you found, confirm with the user, then write.

## The division of artifacts (doctrine, not a question)

Cliban is for work-lifecycle artifacts; the repo is for knowledge that outlives the work. Write this into the adapter as-is — it is the contract's foundation, not a preference to collect:

| Artifact | Home | Why |
|---|---|---|
| Spec / PRD | issue `## Spec` | scoped to the work |
| Implementation plan | issue `## Plan` | nobody needs ticked checkboxes in git history |
| Progress, findings, dead ends | issue `## Activity Log` via `issue log` | timeline, not documentation |
| Durable reusable lessons | project `## Notes` via search-first round-trip | queryable across sessions |
| ADRs, `CONTEXT.md`, domain docs | **the repo**, plaintext, git-tracked | survives contributors, travels with clones |

## Process

### 1. Explore

Read whatever exists; don't assume:

- **Cliban availability** — probe `cliban --help`. If missing, stop; there is nothing to bind. Point the user at https://github.com/LioraLabs/cliban for install (GitHub release binaries, or AUR on Arch), and offer to re-run once it's on `$PATH`.
- **Existing binding** — `docs/agents/issue-tracker.md` already present? Then this is a re-run: load it, present current values, and only ask about what the user wants changed. Re-runs regenerate the adapter, so diff the existing file against the template first and carry every hand-added section or edit forward into the draft unless the user is deliberately changing it — flag anything you carried (or couldn't place) in the draft rather than silently dropping it. Direct edits to the adapter are a blessed workflow, not drift to correct.
- **Board state** — `cliban project ls --json`; match repo basename (case-insensitive) against `key` and `name`.
- **Doc anchors** — `CLAUDE.md` and `AGENTS.md` at the repo root; an existing `## Agent skills` section in either.
- **Monorepo signals** — workspace manifests, populated `packages/*`. Relevant to Section A.

### 2. Present findings and ask

One section, one answer, then the next. Lead each with the recommended answer so the user can accept it in a word.

**Section A — Project binding.**

If exploration matched a cliban project, propose it. Otherwise propose creating one keyed from the repo basename (uppercase, 2-10 chars, letters/digits):

```bash
cliban project add <KEY> "<Repo Name>" --description-file - <<'EOF'
<one-line purpose>

## Notes
EOF
```

Monorepo with genuinely independent packages: offer one project per package only if the user tracks them separately today; default remains one project per repo.

**Section B — Key placement.** Explain rather than ask: the dispatcher puts issue
keys in branch names and integration commits so work remains attributable. Keys
stay out of source code, comments, tests, and docs. This invariant is not
configurable per repo.

**Section C — Dispatcher workspace.** Explain rather than ask: `ticket start`
creates an isolated `.worktrees/<git_branch_name>` worktree for standalone and
milestone work. The dispatcher owns this invariant; it is not configurable per repo.

**Section D — Reviewer.** Who runs the ticket review?

Exploration tells you what's available: a review skill in the installed suites, a `code-reviewer`-style agent type, or neither.

- **an agent type** (recommended when one exists) — name it; the gate dispatches that agent
- **a skill** — name it (`<plugin>:<skill>`); the gate invokes it with the two verdicts as its brief
- **none** (recommended default) — the gate dispatches a general-purpose agent with the inline brief `complete-issue` carries

Say plainly that this only chooses *who* reviews. Non-trivial or risky work gets
one fresh-context review once by default before ready; a plan adds an earlier
checkpoint only where a mistake would compound expensively. Skip this section
if `cliban-flow` isn't installed; nothing else reads it.

### 3. Confirm and write

Show the user a draft of the adapter and the pointer block; let them edit before writing.

**Pick the pointer file:** edit `CLAUDE.md` if it exists, else `AGENTS.md` if it exists, else ask which to create. Never create the one when the other already exists. If an `## Agent skills` block exists, update it in place — don't append a duplicate.

The pointer block:

```markdown
## Agent skills

### Issue tracker

Work is tracked on the local cliban board (project `<KEY>`). See `docs/agents/issue-tracker.md`.
```

The adapter, written to `docs/agents/issue-tracker.md` (create `docs/agents/` if needed):

```markdown
# Issue tracker: cliban

Work for this repo is tracked on the local cliban board, driven by the `cliban` CLI.
Every read takes `--json`. The command surface is specified by the `cliban` skill;
the workflow contract that governs where each artifact lands is the
`cliban-workflow` skill, which ships in the separate `cliban-flow` plugin.

- **Project key:** <KEY>
- **Key policy:** everywhere (dispatcher branches and commits carry keys; source code, comments, tests, and docs do not)
- **Workspace convention:** dispatcher-managed worktree at `.worktrees/<git_branch_name>`
- **Reviewer:** <agent type `<name>` | skill `<plugin>:<skill>` | none — general-purpose agent with the inline brief>

## Where artifacts live

| Artifact | Home |
|---|---|
| Spec / PRD | issue `## Spec` |
| Implementation plan | issue `## Plan` — proportional and optionally structured for `lint`/`tick`/`promote` |
| Progress, findings, dead ends | issue `## Activity Log` via `cliban issue log` |
| Durable reusable lessons | project `## Notes` — search first: `cliban project search <KEY> "<terms>" --json` |
| ADRs, CONTEXT.md, domain docs | this repo, plaintext, git-tracked — never cliban |

Implementation plans are deliberately not git-tracked. ADRs deliberately are.

## When a skill says "publish to the issue tracker"

```bash
cliban issue add "<title>" --project <KEY> --label <bug|feature|refactor|chore> \
  --description-file - --json <<'EOF'
## Spec

<the spec>
EOF
```

## When a skill says "fetch the relevant ticket"

```bash
cliban issue show KEY --json                    # whole issue
cliban issue cat KEY --section spec|plan|activity|notes
cliban issue current --json                     # issue for the current branch
```

## Blocking edges

`--blocks` / `--blocked-by` on `issue add` / `issue edit`. Relations live on the
board — never as `Blocked by:` text lines in repo files.
```

### 4. Done

Tell the user what now reads the adapter: every cliban skill resolves the project
key and reviewer from it, while the dispatcher owns key placement and workspace; the plugin's
SessionStart hook starts injecting live board state (current-branch issue,
in-progress, blocked) into every session opened in this repo — the adapter's
existence is what switches it on. Editing `docs/agents/issue-tracker.md` directly
later is fine; re-running this skill is only for changing a binding or starting over.

If `cliban-flow` is not installed, mention it once: it adds the feature workflow on top of the board — `explore-feature` → `scope-milestone` → `complete-issue` / `complete-milestone` — via `/plugin install cliban-flow@lioralabs`. Nothing here depends on it, so leave it at that.
