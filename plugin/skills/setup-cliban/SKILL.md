---
name: setup-cliban
description: "Bind a repo to its cliban board — project key, key-referencing policy, branch/worktree convention, and which craft stack (superpowers, mattpocock-skills, plan mode) publishes its specs and plans to cliban. Run once per repo; re-run to change the binding."
disable-model-invocation: true
requires_skills: [cliban-workflow]
---

# Setup Cliban

Scaffold the per-repo binding that every cliban workflow skill — and any third-party craft stack — reads:

- **The adapter** — `docs/agents/issue-tracker.md`, declaring cliban as this repo's issue tracker and recording the four bindings (project key, craft stack, key policy, branch convention)
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
- **Installed craft stacks** — a stack is installed if *any* skill with its plugin prefix appears in the available-skills list. Do not conclude a specific member skill is missing just because it's not listed: user-invocable-only skills (`disable-model-invocation: true`) never appear in the model-visible list. In particular, the mattpocock workflow skills (`to-spec`, `to-tickets`, `implement`, `triage`, `wayfinder`) are all invocable only by the user — if you can see `mattpocock-skills:tdd` or `mattpocock-skills:grilling`, the whole suite including `to-spec` is installed.
  - `superpowers:*` → superpowers (brainstorming, writing-plans, subagent-driven-development, using-git-worktrees, finishing-a-development-branch)
  - `mattpocock-skills:*` → the mattpocock suite
  - neither prefix anywhere → the stack is "none" (plan mode + the inline stage actions in `cliban-workflow`)
- **Monorepo signals** — workspace manifests, populated `packages/*`. Only relevant to Section A.

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

**Section B — Craft stack.**

Propose the detected stack. This decides the adapter's stage mapping — which skills own the *craft* of spec/plan/execute/finish, while the cliban contract owns *where the artifacts live*:

- **superpowers** — brainstorming → `## Spec`; writing-plans → `## Plan`; subagent-driven-development executes with `tick`/`log`; finishing-a-development-branch drives the status moves.
- **mattpocock-skills** — reach a design however you like (grilling, plan mode, plain conversation); `to-spec` publishes to `## Spec`; `to-tickets` creates issues with `--blocks` edges; `implement` reads the ticket from cliban, writes `## Plan`, and drives TDD with `tick`/`log`. Their `triage` labels are ordinary cliban labels, and their `/wayfinder` charts big fuzzy efforts as a map issue with decision sub-issues on the board (the adapter's "Wayfinding operations" section, written below, is what wires it up).
- **none** — plan mode for design; the inline stage actions in `cliban-workflow` for everything else.

If both stacks are installed, ask which one owns the rhythm; don't blend them.

If **no stack is detected**, don't silently default — tell the user the choice exists, once and without pressure: "none" is fully supported (plan mode plus the inline stage actions; nothing about cliban degrades), and if they want a craft stack later they can install one and re-run this skill:

- superpowers: `/plugin install superpowers@claude-plugins-official`
- mattpocock-skills: `/plugin install mattpocock-skills@claude-plugins-official`

Do not walk them through an install mid-setup; bind "none" now and let them come back.

**Section C — Key policy.** Where may cliban issue keys (`PROJ-42`) appear in git-tracked artifacts?

- **everywhere** (recommended default) — branch names and commit messages, like any tracker; greppable history
- **branches-only** — keys in branch names (so `issue current` works) but never in commit messages; for repos whose history is public while the board is private
- **never** — keys stay entirely on the board

All three forbid keys in source code, comments, and docs — a tracker key in code rots the moment the board archives it. Only the commit/branch surface varies.

**Section D — Branch convention.** What happens when work starts on an issue?

- **branch-per-issue** (recommended default) — create/switch to the issue's `git_branch_name`
- **worktree-per-issue** — `git worktree add <worktrees-dir>/<git_branch_name>` so parallel issues never share a checkout; ask where worktrees live (`.worktrees/` is the common answer)
- **none** — the user manages branches by hand

(This binds the *solo* flow. `complete-milestone` always uses wave-time worktrees off the milestone branch — orchestration is not configurable per repo.)

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
Every read takes `--json`; the description contract and mutation commands are
specified by the cliban plugin's `cliban-workflow` skill.

- **Project key:** <KEY>
- **Craft stack:** <superpowers | mattpocock-skills | none>
- **Key policy:** <everywhere | branches-only | never> (keys never appear in source code, comments, or docs under any policy)
- **Branch convention:** <branch-per-issue | worktree-per-issue at `<dir>` | none>

## Where artifacts live

| Artifact | Home |
|---|---|
| Spec / PRD | issue `## Spec` |
| Implementation plan | issue `## Plan` — parseable; mutate only via `tick`/`log`/`promote` |
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

## Stage mapping

<the craft-stack paragraph from Section B, expanded with the stack's actual skill names>
```

For the **mattpocock-skills** stack, this file doubles as the `docs/agents/issue-tracker.md` their `setup-matt-pocock-skills` skill would have written for an "other" tracker — do not run their setup's Section A on top of it. Their `/wayfinder` skill consults this doc's "Wayfinding operations" section to learn how the repo's tracker expresses maps, tickets, blocking, and claims — without it, wayfinder falls back to a local-markdown tracker and bypasses the board. So for this stack, **also append**:

```markdown
## Wayfinding operations

Used by `/wayfinder`. The **map** is a cliban issue; its tickets are native sub-issues.

- **Map**: an issue labelled `wayfinder:map`, holding the Destination / Notes /
  Decisions-so-far / Not-yet-specified / Out-of-scope body:
  `cliban issue add "<map name>" --project <KEY> --label wayfinder:map ...`
- **Child ticket**: `cliban issue add --project <KEY> --parent <MAP-KEY> --label wayfinder:<type>`
  (`research` / `prototype` / `grilling` / `task`). List them:
  `cliban issue ls --parent <MAP-KEY> --json`.
- **Blocking**: native edges — `--blocks` / `--blocked-by` on `issue add` / `issue edit`,
  visible on the board. A ticket is unblocked when every blocker is done;
  `cliban issue ls --blocked --project <KEY> --json` lists those still gated.
- **Frontier query**: one call — `cliban issue ls --ready --parent <MAP-KEY> --json`
  (backlog + unblocked + unclaimed IS the frontier).
- **Claim**: `cliban issue claim <TICKET>`, the session's first write (the actor
  defaults to the ambient Claude session, so claims are per-session automatically);
  then `cliban issue mv <TICKET> in-progress`. Release with `cliban issue release`
  if you stop without resolving.
- **Resolve**: post the answer with `cliban issue log <TICKET> "<answer>"`, move it to
  done with a `--note` gist, then index it on the map in one atomic call:
  `cliban issue append-section <MAP-KEY> --section "Decisions so far" "- <name> (<TICKET>) — <gist>"`.
```

### 4. Done

Tell the user what now reads the binding: every workflow skill resolves the project key, key policy, and branch convention from it; `complete-milestone`'s per-ticket agents plan and execute with the bound craft stack; and the plugin's SessionStart hook starts injecting live board state (current-branch issue, in-progress, blocked) into every session opened in this repo — the adapter's existence is what switches it on. Editing `docs/agents/issue-tracker.md` directly later is fine; re-running this skill is only for switching stacks or starting over.
