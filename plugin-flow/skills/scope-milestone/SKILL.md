---
name: scope-milestone
description: "Grill a cliban ticket or milestone until every slicing decision is settled, then fill it with tracer-bullet tickets carrying native blocking edges. Use to scope, slice, ticket, or grill a feature — typically the container explore-feature just created."
requires_skills: [cliban-workflow]
---

# Scope Milestone

Make a container executable: interrogate the design until nothing affecting
the slicing is still assumed, cut it into tracer bullets, publish them with
real dependency edges. `explore-feature` diverges; this converges.

**Load first:** `cliban-flow:cliban-workflow` and `cliban:cliban` — neither
loads on its own.

## 1. Read the container

```bash
cliban milestone show "<name>" --project <KEY> --json   # description = the spec
cliban issue cat <KEY> --section spec                   # or, for a ticket
```

`issue_count: 0` is a fresh milestone waiting to be filled. Non-zero means
tickets already exist — stop and ask whether you're adding to that set. Given
neither a name nor a key, ask; nothing on the board yet is `explore-feature`'s
job. Then read the surroundings: `issue ls --all --json` for collisions,
`project search <KEY> "<terms>" --json` for lessons that constrain the design.

## 2. Grill toward the slice boundaries

The design is a decision tree; the **frontier** is every decision whose
prerequisites are settled. The spec's **Open decisions** are the opening
frontier. Ask the whole frontier in one round, numbered, each with your
recommendation:

```
❓ **Q1** — **<title>**: <the question, with options if it's a choice>
➡️ <your recommended answer, and why>
```

Wait, recompute the tree from the answers, ask the next round. Facts are your
job — look up anything a grep settles, asking the rest of the frontier while a
lookup runs. Decisions are the user's. Prioritise questions that change where
one ticket ends and the next begins; a question that cannot change the
breakdown belongs to the executor — note it and move on. Done when the
frontier is empty.

## 3. Re-test the shape

`explore-feature` guessed the container's shape; re-test it with the answers
in. A milestone, as expected → fill it. A ticket still one slice → step 5's
ticket path. A ticket that outgrew one context window → promote it, saying
why:

```bash
cliban milestone add "<name>" --project <KEY> --description-file - <<'EOF'
<the ticket's spec, plus what the grill settled>
EOF
```

Then resolve the original ticket — first slice (`issue edit <KEY> -m "<name>"`,
spec narrowed) or tracking issue — never left in backlog describing work that
now lives elsewhere, where `issue ls --ready` would offer it to an executor.

## 4. Draft the slices, then quiz

Cut **tracer bullets**: each a narrow but complete path through every layer it
touches, demoable alone, sized to ~100–150 tool calls of execution — sizing is
the biggest cost lever, since an executor's per-turn price grows with its
accumulated context. Prefactor first — make the change easy, then make the
easy change. Give each slice its blocking edges.

**Partition along surface boundaries.** Sibling slices touching the same files
collide at merge in ways git does not mark. Two free signals: the surface each
draft names, and `git log --format= --name-only` for what changes together.
Overlapping unblocked siblings merge, re-slice along the boundary, or take an
explicit blocking edge; a shared surface no test observes always takes edges,
never a shared wave.

**Wide refactors** sequence expand → migrate → contract: add the new form
beside the old, migrate call sites in blast-radius-sized batches (one ticket
each, blocked by the expand, green throughout), then delete the old form in a
ticket blocked by every batch.

Present the breakdown numbered — title, blocked-by, what it delivers end to
end. Ask whether the granularity is right and whether each edge is real. For
small tickets with shared context but no edge, suggest a `related_to` chain
for implementer affinity. **Publish nothing before the user approves.**

## 5. Publish

**Ticket path** — fold the grill's answers into the spec it already has:

```bash
cliban issue edit <KEY> --section spec --description-file - <<'EOF'
**What it delivers:** …
**Acceptance criteria:** …
**Decisions:** what the grill settled, so the executor doesn't reopen it.
EOF
```

**Milestone path** — the milestone already exists; `milestone add` on an
existing name errors. Add issues in dependency order so every `--blocked-by`
names a real key; each `--json` echo carries the new key for the next ticket:

```bash
cliban issue add "<title>" --project <KEY> -m "<milestone name>" \
  --label feature --priority medium --blocked-by <KEY-of-blocker> \
  --description-file - --json <<'EOF'
## Spec

**What it delivers:** the end-to-end behaviour this ticket makes work.

**Acceptance criteria:**
- …

**Decisions:** the grill's answers this ticket has to respect.

## Files

- M path/it/will/edit.rs
- A path/it/will/create.rs
EOF
```

Either path owes the contract:

- **`## Spec` only** — `## Plan` belongs to the executor, written against a
  fresh read of the code.
- **Edges are relations, never prose** — a `Blocked by:` line in a description
  is invisible to `issue ls --ready` and `milestone waves`.
- **`## Files` carries the predicted changeset**, one `A`/`M`/`D` entry per
  path; `milestone waves` intersects these within a wave to catch collisions
  at scope time, and the executor treats it as its leash on exploration.
  Predict, don't guarantee — the executor amends it.
  `issue lint` rejects a malformed entry.
- **No file paths or code in the prose** outside `## Files`, except a snippet
  encoding a decision more precisely than prose can, trimmed to the decision.
- **`--priority medium` explicitly** — the CLI defaults to `none`.

## 6. Show the waves and hand off

Ticket path: report the sharpened ticket and offer `complete-issue`. Milestone
path — let the CLI prove the graph is executable:

```bash
cliban milestone waves --project <KEY> "<milestone name>" --json
```

A cycle exits non-zero naming the issues — fix the edges before handing off.
Non-empty `external_blocked`: say so; finishing waves won't free it. Then
offer `complete-milestone` or `complete-issue`. Starting execution is the
user's call; publishing is where this skill ends.
