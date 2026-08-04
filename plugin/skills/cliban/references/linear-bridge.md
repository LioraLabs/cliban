# The Linear bridge

Two explicit verbs. Nothing syncs in the background, so nothing crosses the
boundary unless you ask.

```bash
cliban import linear ENG-412 --project PROJ            # pull it onto the board
cliban import linear ENG-412 --project PROJ --dry-run  # see it first
cliban import linear --mine --project PROJ             # everything assigned to you
cliban push linear PROJ-42                             # state + progress comment
cliban push linear PROJ-42 --description               # also mirror into the description
cliban push linear PROJ-42 --create --team ENG         # no counterpart yet? make one
cliban sync linear                                     # refresh every linked issue
cliban sync linear --project PROJ                      # ... in one project only
```

**The inbound queue.** `import linear --mine` imports every open Linear issue
assigned to the token's viewer: where the issue's team runs cycles, only the
active cycle counts (the rest is backlog and is reported as skipped); where it
does not, every open assigned issue is in scope. Already-linked issues are
refreshed, not duplicated. `sync linear` re-imports every linked issue in one
call, each under its own link's origin semantics. Both report
created/refreshed/skipped counts and take `--dry-run` / `--json`.

**The living progress comment.** `push` maintains ONE comment per linked
issue and edits it in place (plan ticked/total, recent `issue log` findings,
latest test status) — so your log discipline is exactly what the Linear
thread shows, without notification spam. A comment someone deleted upstream
is recreated once, silently.

Needs `$LINEAR_API_KEY`. Optional `~/.config/cliban/linear.toml` sets the
default team and any state-name overrides; never put the token in it. Set
`push_on_move = true` there and every `issue mv` of a linked issue auto-pushes
state + the living comment after the move lands locally — a failed push warns
on one line and records board activity, but never fails the move.

**Who owns what.** This is the rule that makes the bridge safe to re-run:

| Field | Owner |
|---|---|
| title, priority, labels, due date, workflow state | Linear — a re-import overwrites your local edits |
| `## Spec` | follows who created the pairing: link born from `import` → Linear owns it (re-import refreshes it); link born from `push --create` → cliban owns it (re-import leaves it alone; `push --description` may always mirror it outward) |
| `## Plan`, `## Activity Log`, `## Notes` | cliban — a re-import never touches them |
| Linear description outside cliban's fenced block, Linear comments | humans — never modified |

So: re-import as often as you like, your ticked plan survives. But on an
imported issue, don't edit the title or spec locally and expect it to stick —
change it in Linear. An issue you pushed out with `--create` keeps its local
spec forever.

**Statuses.** `backlog` / `in-progress` / `done` round-trip cleanly.
`blocked` and `in-review` only survive if the Linear team has a column named for
them (Linear types both as "started"), otherwise they collapse into
in-progress. A cancelled Linear issue arrives as `done` **and archived**.

**Gotchas.**

- `push` on an unlinked issue exits 1. Either `--create`, or adopt an existing
  pairing with `import linear ENG-412 --project PROJ --link-to PROJ-42`.
- `push` exits 2 if Linear changed since your last sync. Re-import first, or
  `--force` if you know you are the authority.
- One local issue per Linear issue. A second import refreshes rather than
  duplicating.
