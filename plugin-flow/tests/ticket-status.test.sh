#!/usr/bin/env bash
# CLI-79 — `cliban-flow ticket status <KEY>`: the mergeability gate.
# shellcheck source=plugin-flow/tests/lib.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

# ---------------------------------------------------------------- mergeable

# The ticket branch carries the milestone tip plus its own work.
fixture_new
key=$(new_issue "Ahead of the milestone")
branch=$(branch_of "$key")
gitf checkout -q -b "$branch" milestone/test-milestone
commit_file feature.txt "ticket work"

run_flow ticket status "$key"
assert_status 0 "a branch containing the milestone tip is mergeable"
assert_out_has "mergeable" "the verdict is mergeable"
assert_out_lacks "sync-required" "the mergeable verdict says nothing about syncing"
assert_out_has "milestone/test-milestone" "the verdict names the milestone branch"
assert_board_has "$key" "[cliban-flow] ticket status $key: mergeable" \
    "the mergeable verdict is recorded on the board"

# A branch sitting exactly on the milestone tip is an ancestor of itself.
fixture_new
key=$(new_issue "Level with the milestone")
branch=$(branch_of "$key")
gitf branch "$branch" milestone/test-milestone

run_flow ticket status "$key"
assert_status 0 "a branch level with the milestone tip is mergeable"
assert_out_has "mergeable" "the verdict is mergeable"

finish
