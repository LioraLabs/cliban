---
name: explore-feature
description: "Explore a rough feature idea into a shared design, landing a cliban container: one ticket or an empty milestone. Use before tickets exist, when the user wants to think a feature through or figure out what to build."
requires_skills: [cliban-workflow]
---

# Explore Feature

Diverge on a rough idea until the design is agreed, then publish a
**container** — one issue, or a milestone with no tickets in it yet.
`scope-milestone` grills that container and fills it.

**Load first:** `cliban-flow:cliban-workflow` and `cliban:cliban` — neither
loads on its own.

## 1. Ground yourself

Read the repo areas the idea touches, then the board:

```bash
cliban issue ls --all --json                    # work this could collide with
cliban project search <KEY> "<terms>" --json    # lessons already learned here
```

An issue that already covers this, or a note that settles a question you were
about to ask, changes the conversation — say so rather than re-derive it.

## 2. Diverge

One question per message, multiple choice where it fits. Chase the itch under
the request — a feature ask is usually a proposed solution, and the problem
beneath it often has a cheaper cure. Propose 2–3 approaches with your
recommendation, including the cheap one whose job is making the expensive one
justify itself. Name what is **out of scope** and get agreement — that list
stops the grill slicing tickets nobody asked for.

## 3. Get approval

Present the design, sections scaled to their complexity. Done when it names:

1. The problem, in the user's terms
2. The approach, and what it beat
3. The scope boundary — in, and explicitly out
4. **The open decisions** — noticed and deliberately unsettled; a deliverable,
   the grill's opening frontier

Write nothing to the board before approval. If the user breaks off first,
capture the idea as a backlog issue so it isn't lost, and say so.

## 4. Publish the container

A **milestone** for several ordered slices; **one issue** for a single
demoable change that fits one fresh context window — the test is the context
window, not the calendar. Present it as a guess; `scope-milestone` re-tests it
once the questions are answered.

```bash
# Milestone: spec in the description, NO tickets — issue_count 0 is how
# scope-milestone recognises one waiting to be filled.
cliban milestone add "<name>" --project <KEY> --description-file - <<'EOF'
## Spec

**Problem:** …
**Approach:** … (and what it beat)
**In scope / Out of scope:** …
**Open decisions:** …
EOF
```

```bash
# Or one issue — same four things in ## Spec. --priority medium is explicit
# because the CLI defaults to none.
cliban issue add "<title>" --project <KEY> --label <bug|feature|refactor|chore> \
  --priority medium --description-file - --json <<'EOF'
## Spec
…
EOF
```

Carry the open decisions across **verbatim** — the one part of the design that
dies with the conversation.

## 5. Hand off

Report the milestone name or issue key and offer `scope-milestone`. The
handoff is that name, not this conversation, so the grill can happen in a
fresh session next week.
