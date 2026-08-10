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

# ------------------------------------------------------------ sync-required
#
# The gate is ancestry, not mergeability. These two cases differ in whether a
# merge would conflict and must not differ in the verdict — that pair is what
# pins "strict ancestry, never a trial merge"; either case alone does not.

# Behind, and a merge would be clean: the two sides touch different files.
fixture_new
key=$(new_issue "Behind but conflict-free")
branch=$(branch_of "$key")
gitf checkout -q -b "$branch" milestone/test-milestone
commit_file ticket-side.txt "ticket work"
gitf checkout -q milestone/test-milestone
commit_file milestone-side.txt "another ticket landed"
before=$(gitf rev-parse "$branch")

run_flow ticket status "$key"
assert_status 1 "a branch that is behind is not mergeable"
assert_out_has "sync-required: milestone/test-milestone@" "the verdict is sync-required"
assert_out_has "is not an ancestor of $branch" "the verdict names the ticket branch"
assert_out_has "cliban-flow ticket sync $key" "the verdict names the command that fixes it"
assert_out_has "orchestrator must not" "the verdict says the orchestrator must not resolve it"
assert_board_has "$key" "[cliban-flow] ticket status $key: sync-required" \
    "the sync-required verdict is recorded on the board"
assert_eq "$(gitf rev-parse "$branch")" "$before" "the ticket branch was not moved"
assert_eq "$(gitf status --porcelain)" "" "the working tree was left clean"
assert_eq "$(ls "$FIXTURE_REPO/.git/MERGE_HEAD" 2>/dev/null)" "" "no merge was started"

# The same branch merges cleanly, which is exactly why "no conflicts" is the
# wrong gate: GitHub would have called this one mergeable.
gitf checkout -q "$branch"
gitf merge -q --no-edit milestone/test-milestone
assert_eq "$?" "0" "the behind branch would in fact have merged without conflict"

# Diverged, and a merge would conflict: both sides touch the same file.
fixture_new
key=$(new_issue "Diverged from the milestone")
branch=$(branch_of "$key")
gitf checkout -q -b "$branch" milestone/test-milestone
commit_file shared.txt "the ticket's line"
gitf checkout -q milestone/test-milestone
commit_file shared.txt "the milestone's line"

run_flow ticket status "$key"
assert_status 1 "a diverged branch is not mergeable"
assert_out_has "sync-required: milestone/test-milestone@" "the verdict is sync-required"
assert_out_has "cliban-flow ticket sync $key" "the verdict names the command that fixes it"
assert_eq "$(gitf status --porcelain)" "" "the working tree was left clean"

# ------------------------------------------------------------------- guards
#
# Exit 2 is a different claim from exit 1 and the later subcommands branch on
# the difference: 1 means the branch is behind and syncing fixes it, 2 means the
# question could not be asked. A guard that answered "sync-required" would send
# an agent to run a merge that cannot help.

# No ticket branch yet.
fixture_new
key=$(new_issue "Never started")
branch=$(branch_of "$key")

run_flow ticket status "$key"
assert_status 2 "a missing ticket branch is refused"
assert_out_has "$branch" "the refusal names the branch that is missing"
assert_out_lacks "sync-required" "a missing branch is not reported as sync-required"
assert_board_has "$key" "[cliban-flow] ticket status $key: refused" \
    "the refusal is recorded on the board"

# No milestone branch yet.
fixture_new
key=$(new_issue "Milestone never started")
branch=$(branch_of "$key")
gitf branch "$branch" main
gitf branch -q -D milestone/test-milestone

run_flow ticket status "$key"
assert_status 2 "a missing milestone branch is refused"
assert_out_has "milestone/test-milestone" "the refusal names the milestone branch"
assert_out_lacks "sync-required" "a missing milestone branch is not sync-required"
assert_board_has "$key" "[cliban-flow] ticket status $key: refused" \
    "the refusal is recorded on the board"

# An issue with no milestone has no integration target at all.
fixture_new
key=$(new_issue_no_milestone "Loose ticket")
branch=$(branch_of "$key")
gitf branch "$branch" milestone/test-milestone

run_flow ticket status "$key"
assert_status 2 "a ticket on no milestone is refused"
assert_out_has "milestone" "the refusal says the ticket is on no milestone"
assert_board_has "$key" "[cliban-flow] ticket status $key: refused" \
    "the refusal is recorded on the board"

# A key the board has never heard of.
fixture_new
run_flow ticket status FLOW-404
assert_status 2 "an unknown issue key is refused"
assert_out_has "FLOW-404" "the refusal names the key it could not find"

# Run from outside any repository.
fixture_new
key=$(new_issue "Wrong directory")
FLOW_CWD="$FIXTURE_ROOT"
run_flow ticket status "$key"
unset FLOW_CWD
assert_status 2 "running outside a git repository is refused"
assert_out_has "git repository" "the refusal says where it should have been run"

finish
