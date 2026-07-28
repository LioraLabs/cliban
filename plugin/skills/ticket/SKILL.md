---
name: ticket
description: "Create a cliban issue through conversation. Use when user invokes /ticket to brainstorm and create a well-formed cliban issue."
requires_skills: [cliban-workflow]
---

# Ticket — Create a Cliban Issue

Create a well-formed cliban issue through natural conversation. Starts quick, goes deeper if the topic warrants it.

## Prerequisites

Cliban must be on `$PATH`. The `cliban-workflow` skill handles detection and graceful degradation.

## The Process

### Phase 1: Quick Understanding (2-3 questions)

1. **What are you building/fixing?** — get the core idea in one sentence
2. **What type of work is this?** — feature, bug, refactor, chore (suggest based on description)
3. **How urgent is this?** — map to cliban priority (default `medium`)

Resolve the active project via the convention layer (basename match → fallback to user prompt).

After these questions, draft a ticket:

```
Project: <PROJECT-KEY>
Title: <imperative, concise>
Type: <feature|bug|refactor|chore>  → label
Priority: <none|low|medium|high|urgent>
Description: <2-3 sentences from what you've learned>
```

Ask: **"Does this capture it, or should we dig deeper?"**

### Phase 2: Go Deeper (if needed)

If the user wants more detail, ask follow-up questions **one at a time**:

- What does success look like? (acceptance criteria — these go in the `## Spec` section)
- What context would help someone picking this up? (background, constraints)
- Are there dependencies or blockers? (use `--blocked-by KEY` or `--blocks KEY`)
- Which milestone should this attach to? (use `--milestone NAME`)

Update the draft with a richer description.

### Phase 3: Create the Ticket

Once the user approves the draft, create the issue via stdin so the description can be multi-line:

```bash
cliban issue add --project <KEY> \
  --title "<title>" \
  --priority <priority> \
  --label <type> \
  [--milestone "<name>"] \
  [--blocked-by KEY] \
  [--blocks KEY] \
  --description-file - --json <<'EOF'
## Spec

<description body — at least the core idea; add acceptance criteria if Phase 2 was used>
EOF
```

If the user specified a parent (e.g., "this is under PROJ-10"), add `--parent PROJ-10` instead of (or in addition to) `--milestone`.

Report the created key, title, and git branch name from the JSON response:

```
Created <KEY>: <title>
Priority: <priority> | Project: <project>
Branch: <git_branch_name>
```

### Creating Sub-Tickets

If the user says "under PROJ-10" or "child of PROJ-10", pass `--parent PROJ-10`. Cliban caps depth at 2 — if the target parent is already a sub-issue, fall back to `--related-to PROJ-10` instead and explain why.

## Output

After creation, show exactly:

```
Created <KEY>: <title>
Priority: <priority> | Project: <project>
Branch: <git_branch_name>
```

If it's a sub-ticket, also show: `Parent: <PARENT-KEY>`.
