# AGENTS.md

## Where shared decisions live

`cliban-core` is the bottom of the workspace: everything depends on it, it
depends on none of them. Shared decisions go there.

A *decision* is a question more than one place has to answer the same way —
what the valid statuses are, where a `## Section` ends, how issues sort. If the
CLI, the TUI, and the Linear bridge could each answer it, import the answer
from `cliban-core` instead of re-spelling it locally. Two implementations of
one decision are two chances for them to disagree, and the disagreement is
always a bug rather than a preference.

`cliban_core::sections` is the worked example: it lives in core rather than in
the CLI's `descmd` because a Linear re-import replaces `## Spec` and must not
disturb the `## Plan` an agent has been ticking. Two answers to "where does
this section end" would silently eat a plan.

## Agent skills

### Issue tracker

Work is tracked on the local cliban board (project `CLI`). See `docs/agents/issue-tracker.md`.
