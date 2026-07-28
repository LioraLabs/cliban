---
name: cliban-workflow
description: "Convention layer for cliban-based workflow management. Loaded by workflow skills via requires_skills to provide cliban command vocabulary, status mapping, and the parseable-description contract."
---

# Cliban Workflow — Convention Layer

This skill is loaded automatically by workflow skills that declare `requires_skills: [cliban-workflow]`. It teaches when and how to use the cliban CLI for the brainstorm → plan → execute → finish workflow.

## Detection and Graceful Degradation

Before performing ANY cliban action, check availability:

1. **Probe `cliban --help`.** If the command is not on `$PATH` (non-zero exit / "command not found"), skip all cliban actions silently for this session. Do not warn, do not suggest install, do not block the workflow.
2. **If the probe succeeds, attempt the first real cliban call.** If it fails (DB missing, schema mismatch, exit 3), surface the error once with `"cliban error: <message> — try 'cliban init' or check $CLIBAN_DB"` and then skip remaining cliban actions this session. Do not retry.

<IMPORTANT>
Cliban integration is REQUIRED for the new workflow but the SKILLS must still function for users who haven't installed cliban yet. Workflow skills fall back to local-file behavior only if explicitly directed; otherwise they error clearly with the cliban setup instruction above.
</IMPORTANT>

## Vocabulary

Cliban's primitives are:

- **Project** — top-level scope. Identified by uppercase key (e.g. `ACME`, `BLOG`).
- **Milestone** — bundle of issues. Named per project, optional target date.
- **Issue** — body of work. Key shape: `{PROJECT}-{N}` (e.g. `PROJ-12`).
- **Sub-issue** — depth-limited to 2. Use `--parent KEY` on `issue add`.
- **Labels** — free-form per project (auto-created on first use).
- **Relations** — `blocks`, `blocked_by`, `related_to` (symmetric).
- **Project memory** — lifecycle-free durable knowledge stored as `###` subsections under the project description's `## Notes`.

## Project Notes — Progressive Memory

The project description's `## Notes` is the store for reusable lessons that should survive a ticket: repository conventions, non-obvious tool behavior, recurring hazards, and decisions whose rationale will matter again. It is not a second activity log, a status feed, or a dump of the current session. Store one independently useful lesson per descriptive `###` subsection.

Probe memory support once with `cliban project search --help`. If it is unavailable, skip project-memory search and recording for the session while continuing all project, milestone, and issue workflows normally. This means the installed CLI predates progressive project memory; it is not a general cliban failure.

At the start of non-trivial work, derive a few task-specific keywords and fuzzy-search project memory before planning or implementation:

```bash
cliban project search <KEY> "<specific terms>" --section notes --json
```

The search requires every whitespace-separated term to fuzzy-match within a subsection and returns only matching `###` blocks, ranked and bounded. Read and surface only relevant hits. Do not load the whole notes section or add an always-loaded memory hook. Use `cliban project show <KEY> --section notes` only for deliberate inventory or editing.

Record memory only after discovering a durable, reusable lesson. Search before updating to avoid duplicate or contradictory subsections. Project descriptions update as a whole, so round-trip the current description and preserve every existing section:

```bash
cliban project search <KEY> "<lesson keywords>" --section notes --json
cliban project show <KEY> --json | jq -r '.description' > /tmp/project.md
# Add or update one `### <concise topic>` under `## Notes` in /tmp/project.md.
cliban project edit <KEY> --description-file /tmp/project.md
```

If an existing subsection covers the lesson, update it instead of creating a near-duplicate. Remove obsolete knowledge; when a decision changes, preserve the current truth rather than accumulating a chronology.

## Status Mapping

| Workflow event | Cliban status |
|---|---|
| Plan written | `backlog` |
| First step picked up | `in-progress` |
| Stuck on dependency | `blocked` |
| PR opened | `in-review` |
| PR merged / local merge | `done` |
| Discarded / abandoned | keep current status, append log entry |

## Active-Project Resolution

When a workflow skill needs a project context:

1. Try `basename $(git rev-parse --show-toplevel)` and match (case-insensitive) against `cliban project ls --json` results (both `key` and `name`).
2. If no match, list projects and ask the user which one — or whether to create a new project.

```bash
REPO=$(basename "$(git rev-parse --show-toplevel)" 2>/dev/null | tr '[:lower:]' '[:upper:]')
cliban project ls --json | jq --arg r "$REPO" 'select(.key==$r or (.name|ascii_upcase)==$r)'
```

## Active-Issue Resolution

When a workflow skill needs the current issue:

1. Try `cliban issue current --json` (reads current git branch, parses the cliban-style prefix).
2. If exit code 1, ask the user for the issue KEY.

## Parseable-Description Contract

Issue (and milestone/project) descriptions follow a strict markdown contract that several cliban commands parse:

```markdown
## Spec

[brainstorming output — free-form markdown]

## Plan

### Task 1: short name

**Files:** exact paths

**Behaviors:** observable outcomes and edge cases

**Test intent:** what must fail before implementation and what the tests prove

- [ ] Add the failing behavior tests and verify the expected failure.
- [ ] Implement the behavior within the listed boundaries.
- [ ] Run focused and broader verification.
- [ ] Commit the coherent change.

### Task 2: short name
...

### Review Checkpoint: scope of the group above

### Task 3: short name
...

## Activity Log

- 2026-05-20T13:42Z — chronological entry
- 2026-05-21T09:15Z — another entry

## Notes

[for projects: durable reusable lessons, one per descriptive H3 subsection; for issues and milestones: node-local context]
```

Binding conventions:

1. Top-level anchors: `## Spec`, `## Plan`, `## Activity Log`, `## Notes`. Exact-match.
2. Plan tasks: H3 `### Task <N>: <name>`. Numbered uniquely.
3. Plan steps: GFM checkbox lines at column zero (`- [ ] ...` or `- [x] ...`). Indented child bullets are NOT steps.
4. Review checkpoints: H3 `### Review Checkpoint: <scope>`. No steps, no number — a marker between task groups telling the executor where to batch its review. `tick`/`promote` ignore them.
5. Promotion suffix: a step pointing to a separate issue is rewritten as `- [ ] Step 3: CSRF middleware → PROJ-18`.
6. Strict failure: structural violations exit with code 2 — fix the description and retry, no best-effort recovery.

## Mutation Commands (atomic via SQLite)

```bash
# Read one section without round-tripping the whole description:
cliban issue show KEY --section spec|plan|activity|notes

# Atomically flip a plan step's checkbox:
cliban issue tick KEY --task N --step M --json

# Atomically append a timestamped Activity Log entry:
cliban issue log KEY "<message>" --json
cliban issue log KEY --message-file - --json  # stdin

# Promote a step into its own issue and rewrite the step line:
cliban issue promote KEY --task N --step M --title "..." --as sub-issue|related --json
```

Each of these runs in a single SQL transaction. Concurrent calls are serialized.

## Cross-Project Conventions

- **Canonical labels** for `--label`: `bug`, `feature`, `refactor`, `chore`. Cliban auto-creates labels on `issue add --label`; do not pre-create. Orphan labels are not garbage-collected, so prefer the canonical set.
- **Default priority** on issue creation: `medium`. Use `high` / `urgent` only when explicitly indicated.
- **Relations:** use `--blocks` / `--blocked-by` for hard dependencies, `--related-to` for soft references.
- **Promotion-mirror responsibility:** when a promoted child issue moves to `done`, the workflow skill that did the move is responsible for also calling `cliban issue tick` on the referencing step in the parent. Cliban core does NOT auto-mirror — this is the skill's job, deliberately kept out of cliban to avoid coupling the core to the description-parsing contract.

## Workflow Actions by Stage

Stages map onto the [superpowers](https://github.com/obra/superpowers) plugin's skills when it is installed (`superpowers:brainstorming`, `superpowers:writing-plans`, `superpowers:executing-plans`, `superpowers:subagent-driven-development`, `superpowers:finishing-a-development-branch`). Use those skills for the *craft* of each stage; this contract governs *where the artifacts live* — sections of the cliban node description — and supersedes any file-based storage those skills describe. Without superpowers, perform the stage directly with the actions below.

### Brainstorming / spec
- Detect active project (above)
- Ask scope: project / milestone / issue
- Create the appropriate node with the `## Spec` section in its description

### Planning
- Take or infer an Issue key
- Read spec: `cliban issue show KEY --section spec`
- Write plan via `cliban issue edit KEY --description-file -` (round-trips full description preserving Spec + Activity Log)

### Executing
- `cliban issue mv KEY in-progress`
- For each step: execute → `cliban issue tick KEY --task N --step M`
- For bugs: `cliban issue add --label bug --blocks KEY` + `cliban issue log KEY "bug surfaced: NEWKEY"`
- For oversized steps: `cliban issue promote KEY --task N --step M --title "..." --as sub-issue`

### Ticket
- `cliban issue add --project KEY --title "..." --priority ...`

### Bugs
- Add: `cliban issue add --label bug --priority ...`
- List: `cliban issue ls --label bug --json`
- Resolve: `cliban issue mv KEY done`

### Status
- `cliban project ls --json`
- `cliban issue ls --status in-progress --json`
- `cliban issue blocked --json`

### Finishing a branch
- PR opened: `cliban issue mv KEY in-review` + `cliban issue log KEY "PR opened: <url>"`
- Local merge: `cliban issue mv KEY done`
- Discard: `cliban issue log KEY "work discarded"` (keep current status)

## What NOT to Do

- Don't parse the human table output of `ls`/`show`. Always use `--json`.
- Don't nest sub-issues three levels deep — cliban exits 2 (use `related_to` instead).
- Don't mutate the structured sections (`## Plan`, `## Activity Log`) outside of `tick`/`promote`/`log`. Hand-editing breaks the contract and the next mutation command exits 2.
- Don't pre-create labels — `issue add --label X` auto-creates.
- Don't pass `--editor` in an agent context — exits 2 without a TTY.
- Don't write spec or plan content to plan/spec files in project repos (e.g. superpowers' `docs/` locations). Under this workflow, specs and plans live in the cliban node description.
- **Never write a cliban issue key into source code, comments, commit messages, or any committed artifact.** A cliban key (e.g. `PROJ-42`) is private local tracking metadata — meaningless to anyone reading the repo. Track the work *in cliban* (`tick`/`log`); the key stays out of the code. (A global pre-commit hook enforces this and will block such commits.)
