---
name: explore-feature
description: "Explore a rough feature idea into a shared design, then land it on the cliban board as a ticket or an empty milestone. Use at the start of a new feature, before tickets exist — when the user wants to think something through, kick an idea around, or figure out what to build."
requires_skills: [cliban-workflow]
---

# Explore Feature

Diverge on a rough idea until the design is agreed, then publish a **container** for it: one issue, or a milestone with no tickets in it yet. `scope-milestone` grills that container and fills it.

## 1. Ground yourself

Read the repo areas the idea touches. Then read the board — this is the part that isn't obvious:

```bash
cliban issue ls --json                          # open work this could collide with
cliban project search <KEY> "<terms>" --json    # lessons already learned here
```

An issue that already covers this, or a project note that settles a question you were about to ask, changes the conversation. Say so rather than re-deriving it.

## 2. Diverge

One question per message, multiple choice where it fits. Chase the itch under the request — a feature ask is usually a proposed solution, and the problem beneath it often has a cheaper cure.

Propose 2–3 approaches with your recommendation. Include the cheap one you don't recommend; its job is to make the expensive one justify itself.

Name what is **out of scope** and get agreement. That list is what stops the grill slicing tickets nobody asked for.

## 3. Get approval

Present the design, sections scaled to their complexity, checking in as you go. It is done when it names:

1. The problem, in the user's terms
2. The approach, and what it beat
3. The scope boundary — in, and explicitly out
4. **The open decisions** — what you both noticed and deliberately left unsettled

The fourth is a deliverable, not a failure: it becomes the grill's opening frontier.

Write nothing to the board before the user approves. If they break off first, capture the idea as a backlog issue so it isn't lost, and say so.

## 4. Publish the container

**A milestone** when the work needs several ordered slices; **one issue** when it is a single demoable change that fits one fresh context window. The test is the context window, not the calendar.

Present it as a guess, not a verdict — `scope-milestone` re-tests it once the questions are answered, and promotes a ticket that turned out bigger.

```bash
# Milestone: spec in the description, and NO tickets. issue_count 0 is how
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
# Or one issue — same four things, into ## Spec.
cliban issue add "<title>" --project <KEY> --label <bug|feature|refactor|chore> \
  --priority medium --description-file - --json <<'EOF'
## Spec
…
EOF
```

`--priority medium` is explicit because the CLI defaults to `none`.

Carry the open decisions across **verbatim** — they are the one part of the design that dies with the conversation.

## 5. Hand off

Report the milestone name or issue key, and offer `scope-milestone`. The handoff is that name, not this conversation, so the grill can happen in a fresh session next week.
