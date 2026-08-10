#!/usr/bin/env bash
# CLI-79 — `cliban-flow ticket status <KEY>`: the mergeability gate.
# shellcheck source-path=SCRIPTDIR
# shellcheck source=lib.sh
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
assert_stdout_is "mergeable" "the verdict, alone, is on stdout"
assert_out_lacks "sync-required" "the mergeable verdict says nothing about syncing"
assert_stderr_has "milestone/test-milestone" "the guidance on stderr names the milestone branch"
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

msha=$(gitf rev-parse --short milestone/test-milestone)
run_flow ticket status "$key"
assert_status 1 "a branch that is behind is not mergeable"
assert_stdout_is "sync-required: milestone/test-milestone@$msha is not an ancestor of $branch" \
    "the verdict, alone, is on stdout and names both sides"
assert_stderr_has "cliban-flow ticket sync $key" "the guidance names the command that fixes it"
assert_stderr_has "orchestrator must not" "the guidance says the orchestrator must not resolve it"
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
assert_out_has "on no milestone" "the refusal says the ticket is on no milestone"
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

# A ref that is not a local branch does not satisfy the milestone-branch guard.
fixture_new
key=$(new_issue "Tag is not a branch")
branch=$(branch_of "$key")
gitf branch -q -D milestone/test-milestone
gitf tag milestone/test-milestone main
gitf branch "$branch" main

run_flow ticket status "$key"
assert_status 2 "a tag named like the milestone branch does not satisfy the guard"
assert_out_lacks "mergeable" "no verdict is given against a tag"

# A tag alongside the branch of the same name. git disambiguates a bare name
# tags-before-branches, so any lookup that is not explicitly refs/heads answers
# about the tag — and here the tag is at the commit the branch has moved off,
# which turns a branch that is behind into a false `mergeable`.
fixture_new
key=$(new_issue "Tag shadows the milestone branch")
branch=$(branch_of "$key")
gitf checkout -q -b "$branch" milestone/test-milestone
commit_file ticket-side.txt "ticket work"
gitf checkout -q milestone/test-milestone
stale=$(gitf rev-parse HEAD)
commit_file milestone-side.txt "another ticket landed"
gitf tag milestone/test-milestone "$stale"

run_flow ticket status "$key"
assert_status 1 "a tag shadowing the milestone branch cannot produce a false mergeable"
assert_out_lacks "mergeable" "the stale tag is not mistaken for the branch"

# Trailing arguments are a typo, not something to ignore silently — and a
# refusal reached before the ticket is loaded has nowhere to record itself, so
# it must leave the board alone rather than half-write to it.
fixture_new
key=$(new_issue "One key only")
branch=$(branch_of "$key")
gitf branch "$branch" milestone/test-milestone

run_flow ticket status "$key" --json
assert_status 2 "an unrecognised trailing argument is refused"
assert_out_has "--json" "the refusal names the argument it did not understand"
assert_board_lacks "$key" "[cliban-flow]" "a refusal before the ticket loads writes no board line"

# ------------------------------------------------------- board unavailability
#
# The board is the audit trail, not the authority. A board that cannot be
# written must degrade to a warning, because the alternative is a gate whose
# answer depends on whether logging worked.

fixture_new
key=$(new_issue "Board is down")
branch=$(branch_of "$key")
gitf checkout -q -b "$branch" milestone/test-milestone
commit_file feature.txt "ticket work"
break_board_writes

run_flow ticket status "$key"
assert_status 0 "a failed board write does not change the verdict"
assert_stdout_is "mergeable" "the verdict is unchanged"
assert_stderr_has "could not record" "the failed board write is reported"
unset FLOW_PATH

# ------------------------------------------------------- unreadable board JSON
#
# The dangerous failure is not a crash but a quiet exit 1: callers read that as
# sync-required and go and run a merge. A read that cannot complete must refuse.

fixture_new
key=$(new_issue "JSON reader is broken")
branch=$(branch_of "$key")
gitf checkout -q -b "$branch" milestone/test-milestone
commit_file feature.txt "ticket work"
break_json_reader

run_flow ticket status "$key"
assert_status 2 "a JSON read that fails is refused, not reported as a verdict"
assert_out_lacks "sync-required" "a failed read is never mistaken for sync-required"
assert_out_has "could not read $key" "the refusal says what it could not do"
unset FLOW_PATH

fixture_new
key=$(new_issue "JSON reader is missing")
branch=$(branch_of "$key")
gitf branch "$branch" milestone/test-milestone
hide_json_reader

run_flow ticket status "$key"
assert_status 2 "a missing python3 is refused"
assert_out_has "python3" "the refusal names the missing dependency"
unset FLOW_PATH

# -------------------------------------------------------------- slug agreement
#
# The milestone branch name is derived by reimplementing cliban's own slug
# rules, so the two must be pinned together. cliban's answer is read back off an
# issue's git_branch_name rather than recomputed here.

fixture_new
awkward='Deterministic  integration!! (v2)'
slug=$(cliban_slug_of "$awkward")
cb milestone add "$awkward" --project FLOW >/dev/null
key=$(new_issue "On an awkwardly named milestone" "$awkward")
branch=$(branch_of "$key")
gitf branch "milestone/$slug" main
gitf branch "$branch" "milestone/$slug"

run_flow ticket status "$key"
assert_status 0 "the derived milestone branch is the one cliban's rules produce"
assert_stdout_is "mergeable" "the verdict is mergeable, so the branch was found"

finish
