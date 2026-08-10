#!/usr/bin/env bash
# CLI-80 — `cliban-flow milestone start <NAME>`: the milestone's git layout.
#
# The subcommand under test creates branches and worktrees, so every case runs
# against a throwaway repo under `mktemp -d` with CLIBAN_DB inside it. Nothing
# here may name a path outside $FIXTURE_ROOT.
# shellcheck source-path=SCRIPTDIR
# shellcheck source=lib.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

# The path the subcommand is expected to derive: the primary checkout's own
# path with the milestone slug appended, so the milestone worktree is a sibling
# of the primary checkout and never a directory inside it.
milestone_wt() { printf '%s' "$FIXTURE_ROOT/repo-test-milestone"; }

# ------------------------------------------------------------ the happy path

# fixture_new ships the milestone branch already made; this case is about
# creating it, so it starts from a repo that has only main.
fixture_new
gitf branch -D milestone/test-milestone >/dev/null
main_sha=$(gitf rev-parse main)

run_flow milestone start "Test milestone" -p FLOW
assert_status 0 "starting a milestone succeeds"
assert_stdout_is "$(milestone_wt)" "the worktree path, alone, is on stdout"
assert_eq "$(gitf rev-parse milestone/test-milestone)" "$main_sha" \
    "the milestone branch is created at main's tip"
assert_eq "$(git -C "$(milestone_wt)" rev-parse --abbrev-ref HEAD)" \
    "milestone/test-milestone" "the milestone branch is checked out in that worktree"
assert_eq "$(gitf rev-parse --abbrev-ref HEAD)" "main" \
    "the primary checkout is left on what it was on"
assert_eq "$(gitf status --porcelain)" "" "the primary checkout is left clean"
assert_stderr_has "milestone/test-milestone" "the narration on stderr names the branch"
assert_stderr_has "$(milestone_wt)" "the narration names the worktree it is about to add"
assert_milestone_board_has "Test milestone" \
    "[cliban-flow] milestone start Test milestone: created" \
    "the milestone's own activity log records the line"

finish
