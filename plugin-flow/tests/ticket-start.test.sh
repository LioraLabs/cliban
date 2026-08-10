#!/usr/bin/env bash
# CLI-80 — `cliban-flow ticket start <KEY>`: a ticket worktree under the
# milestone worktree.
#
# Same rule as every suite here: throwaway repo under `mktemp -d`, CLIBAN_DB
# inside it, no path named outside $FIXTURE_ROOT.
# shellcheck source-path=SCRIPTDIR
# shellcheck source=lib.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

# ------------------------------------------------------------ the happy path

fixture_new
fixture_milestone_worktree
# Work integrated into the milestone after its worktree was made: the ticket
# has to branch off the tip as it is now, not off where the milestone started.
commit_file_at "$(fixture_milestone_wt)" integrated.txt "an earlier ticket landed"
tip=$(gitf rev-parse milestone/test-milestone)
key=$(new_issue "A ticket to start")
branch=$(branch_of "$key")

run_flow ticket start "$key"
assert_status 0 "starting a ticket succeeds"
assert_stdout_is "$(fixture_milestone_wt)/.worktrees/$branch" \
    "the worktree path, alone, is on stdout, rooted under the milestone worktree"
assert_eq "$(gitf rev-parse "$branch")" "$tip" \
    "the ticket branch starts at the milestone branch's current tip"
assert_eq "$(git -C "$(fixture_milestone_wt)/.worktrees/$branch" rev-parse --abbrev-ref HEAD)" \
    "$branch" "the ticket branch is checked out in that worktree"
assert_eq "$(gitf rev-parse --abbrev-ref HEAD)" "main" \
    "the primary checkout is left on what it was on"
assert_eq "$(gitf status --porcelain)" "" "the primary checkout is left clean"
assert_stderr_has "cliban-flow: creating $branch" \
    "the script announces the branch it is about to create"
assert_board_has "$key" "[cliban-flow] ticket start $key: created" \
    "the ticket's activity log records the line"

finish
