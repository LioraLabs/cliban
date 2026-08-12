# plugin-tests — behavioral tests for the cliban plugin skills

Skills are prompts; their unit tests are behavioral. Each scenario hands a fresh
subagent a realistic task against a **hermetic throwaway board** (`CLIBAN_DB`
pointed at a tempfile) and then asserts on the board state the agent left
behind — never on transcript prose. Two cliban properties make this work:

- `--db` / `$CLIBAN_DB` gives per-scenario fixture isolation
- `cliban activity --json` is a complete event log of what the agent actually
  did, so assertions can check lifecycle invariants without mirroring the
  agent's implementation cadence

## Layout

Each scenario is a directory under `scenarios/`:

- `seed.sh <db>` — builds the fixture board (must pass `--db "$1"` on every call)
- `prompt.md` — the task, written as a user would say it; it must NOT name the
  skill under test (triggering is part of what's being tested)
- `assert.sh <db>` — exit 0 = pass; prints one `FAIL: ...` line per violation

## Running

Invoke the `test-plugin-skills` project skill from a Claude Code session **in
this repo with the dev plugins loaded** — scenario agents must see both
`plugin/skills/` and `plugin-flow/skills/` under test, or you are testing the
raw model instead of the skills. The runner seeds each fixture, dispatches one
subagent per scenario in parallel, runs the assertions, and checks the real
board's activity feed for leaked writes.
