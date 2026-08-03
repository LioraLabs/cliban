---
name: test-plugin-skills
description: "Run the plugin's behavioral test scenarios (plugin-tests/scenarios/) against fresh subagents on hermetic throwaway boards, assert on resulting board state, and report a pass/fail table. Dev-only; use when asked to test, verify, or regression-check the plugin skills."
---

# Test Plugin Skills

Each scenario in `plugin-tests/scenarios/<name>/` is `seed.sh <db>` + `prompt.md` + `assert.sh <db>`. You are the runner: seed, dispatch one subagent per scenario, assert, report. Assertions judge **board state**, never transcript prose.

## Preconditions

1. Run from the cliban repo with the **dev** plugin skills loaded (the tree under `plugin/skills/` must be what subagents see — otherwise you are testing the installed release, or worse, the raw model). If you cannot confirm the dev skills are loaded, say so in the report header rather than silently proceeding.
2. `cliban` and `jq` on `$PATH`.

## Procedure

**1. Record the leak sentinel.** Before dispatching anything:

```bash
START=$(date -u +%Y-%m-%dT%H:%M:%SZ)
```

**2. Seed every scenario.** For each scenario (all of them, or the ones the user named):

```bash
TMP=$(mktemp -d) && git -C "$TMP" init -q
DB="$TMP/board.db"
bash plugin-tests/scenarios/<name>/seed.sh "$DB"
```

**3. Dispatch all scenario agents in parallel** (isolated DBs — one message, one Agent call per scenario, `general-purpose`). Each agent's prompt is exactly this envelope plus the scenario's `prompt.md` verbatim:

```
Your working directory is <TMP> (a git repo). The cliban board for this work
lives at <DB>. FIRST run: export CLIBAN_DB=<DB>
Every cliban command must hit that database and no other; never pass --db,
never touch the default board. Work autonomously; do not ask questions.

<contents of prompt.md>

When done, reply with one line stating what you did to the board.
```

Do NOT name any skill, quote any skill content, or hint at the expected board
operations — natural triggering and contract-conformant behavior are exactly
what is under test.

**4. Assert.** When each agent finishes:

```bash
bash plugin-tests/scenarios/<name>/assert.sh "$DB"; echo "exit: $?"
```

Exit 0 = PASS. Otherwise FAIL; keep every `FAIL:` line for the report.

**5. Check for leaks into the real board:**

```bash
cliban activity --since "$START" --json
```

Any event here means a scenario agent escaped its `CLIBAN_DB` — mark that run
LEAKED regardless of its assertions (attribute by key/project if possible),
and flag it loudly: the envelope or the skill under test failed containment.

**6. Report** a table: scenario | PASS / FAIL / LEAKED | failure lines. Then
clean up the tempdirs. On failures, quote the relevant board state (the
offending `issue show` / `--section` output), and only then hypothesize which
skill text caused it — the fix belongs in `plugin/skills/`, never in the
scenario, unless the scenario's assertion is itself proven wrong.

## Writing a new scenario

- Seed the *minimum* board that makes the task sensible.
- Write `prompt.md` as a user would actually talk: mention the work, not the tooling.
- Assert the contract, not the wording: counts, labels, sections that must parse (`--section X` exit codes), events that must / must not appear in the activity feed.
- One behavior per scenario. A scenario that can fail for three unrelated reasons is three scenarios.
