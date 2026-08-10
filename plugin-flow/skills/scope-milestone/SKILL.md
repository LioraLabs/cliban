---
name: scope-milestone
description: "Grill a cliban ticket or milestone until every decision that changes the slicing is settled, then fill it with tracer-bullet tickets carrying native blocking edges. Use when the user wants a feature scoped, sliced, ticketed, grilled, or turned into a milestone — typically on the container explore-feature just created."
requires_skills: [cliban-workflow]
---

# Scope Milestone

Take a container off the board and make it executable: interrogate the design until nothing affecting the slicing is still assumed, cut it into tracer bullets, publish them with real dependency edges. `explore-feature` diverges; this converges.

## 1. Read the container

You are handed a milestone name or an issue key. Read it first — it carries the design, and the conversation that produced it may be long gone.

```bash
cliban milestone show "<name>" --project <KEY> --json   # description = the spec
cliban issue show <KEY> --json                          # or, for a ticket
cliban issue cat <KEY> --section spec
```

`issue_count: 0` means a fresh milestone waiting to be filled. **Non-zero means someone already put tickets here** — stop and ask whether you're adding to that set, rather than publishing a second overlapping batch.

Given neither a name nor a key, ask. If the user wants to scope from a conversation with nothing on the board yet, that's `explore-feature` — offer it rather than improvising a container.

Then read the surroundings: `issue ls --json` for collisions, `project search <KEY> "<terms>" --json` for lessons that constrain the design.

## 2. Grill toward the seams

Interrogate the design as a **tree**: every decision branches into the decisions hanging off it. The **frontier** is every decision whose prerequisites are settled — what's answerable now, without guessing at answers you haven't heard.

The spec's **Open decisions** are your opening frontier. Start there rather than re-deriving it.

Ask the whole frontier in one round, numbered, each with your recommendation:

```
❓ **Q1** — **<title>**: <the question, with options if it's a choice>

➡️ <your recommended answer, and why>
```

Then wait. Each round's answers reshape the tree; recompute and ask the next. A question depending on another still open *this* round belongs to a later one.

- **Facts are your job.** Anything the environment can answer — a file's contents, an API's shape, what the schema does today — go look up. Never bill the user for what a `grep` would settle. A lookup in flight is an unsettled prerequisite: ask the rest of the frontier while it runs.
- **Decisions are the user's.** Put each one to them and wait.

**Grill toward the seams.** Prioritise questions whose answers change *where the seam between two tickets falls* — what can land independently, what must exist before what. A question that cannot change the breakdown or its edges is a question for the executor: note it and move on.

Done when the frontier is empty.

## 3. Re-test the shape

`explore-feature` guessed the container's shape before these questions were answered. Check the guess now that they are — same test, better information: can an executor finish this as one coherent change without running out of context?

- **A milestone, as expected** → fill it.
- **A ticket that is still one slice** → skip to step 5's ticket path.
- **A ticket that outgrew one context** → promote it. Say so and why; the extra scope you found is worth hearing.

```bash
cliban milestone add "<name>" --project <KEY> --description-file - <<'EOF'
<the ticket's spec, plus what the grill settled>
EOF
```

Then resolve the original ticket — either it becomes the milestone's first slice (`issue edit <KEY> -m "<name>"`, spec narrowed), or it stays as the tracking issue. Never leave it in backlog describing work that now lives elsewhere: `issue ls --ready` will offer it to an executor.

## 4. Draft the slices, then quiz

Cut **tracer bullets**: each a narrow but *complete* path through every layer it touches, demoable on its own, sized to one fresh context. Prefactoring first — make the change easy, then make the easy change. Give each its blocking edges.

**Wide refactors are the exception.** A mechanical change whose blast radius fans across the codebase can't land green as a tracer bullet. Sequence it **expand → migrate → contract**: add the new form beside the old, migrate call sites in batches sized by blast radius (one ticket each, blocked by the expand, green throughout because the old form still exists), then delete the old form in a ticket blocked by every batch.

Present the breakdown numbered — title, blocked-by, what it delivers end to end. Ask whether the granularity is right, whether each edge is real or just a habit of ordering, and whether anything should merge or split. **Publish nothing before the user approves.**

## 5. Publish

**Ticket path** — nothing to create; fold the grill's answers into the spec it already has:

```bash
cliban issue edit <KEY> --section spec --description-file - <<'EOF'
**What it delivers:** …
**Acceptance criteria:** …
**Decisions:** what the grill settled, so the executor doesn't reopen it.
EOF
```

**Milestone path** — the milestone already exists (from `explore-feature`, or step 3). Do not create it again; `milestone add` on an existing name errors. Add issues **in dependency order** so every `--blocked-by` can name a real key:

```bash
cliban issue add "<title>" --project <KEY> -m "<milestone name>" \
  --label feature --priority medium --blocked-by <KEY-of-blocker> \
  --description-file - --json <<'EOF'
## Spec

**What it delivers:** the end-to-end behaviour this ticket makes work.

**Acceptance criteria:**
- …

**Decisions:** the grill's answers this ticket has to respect.
EOF
```

Each `issue add --json` echoes the new key — read it for the next ticket's `--blocked-by` instead of re-listing the milestone.

Four things this stage owes the contract, on either path:

- **`## Spec` only**, via `--section spec`. `## Plan` belongs to the executor, written against a fresh read of the code.
- **Edges are relations, never prose.** A `Blocked by:` line in a description is invisible to `issue ls --ready` and `milestone waves`, which is the entire reason the edges exist.
- **No file paths or code snippets** — they go stale fastest. Exception: a snippet encoding a decision more precisely than prose can (a schema, a state machine, a type shape), trimmed to the decision.
- **`--priority medium` explicitly.** The CLI defaults to `none`.

## 6. Show the waves and hand off

On the ticket path there's no graph — report the sharpened ticket and offer `complete-issue`.

On the milestone path, let the CLI prove the graph is executable:

```bash
cliban milestone waves --project <KEY> "<milestone name>" --json
# Waves: [CLI-12] -> [CLI-13, CLI-15] -> [CLI-14]
```

A cycle exits non-zero naming the issues — fix the edges before handing off. A non-empty `external_blocked` means something outside the milestone gates this work; say so, because finishing waves won't free it.

Then offer `complete-milestone` (the whole thing, one agent per ticket, in wave order) or `complete-issue` (one ticket now). Starting execution is the user's call; publishing is where this skill ends.
