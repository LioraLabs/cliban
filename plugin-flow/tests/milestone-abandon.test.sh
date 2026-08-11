#!/usr/bin/env bash
# CLI-93 — explicit teardown for a whole milestone.
# shellcheck source-path=SCRIPTDIR
# shellcheck source=lib.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

fixture_new
fixture_milestone_worktree
first=$(new_issue "First abandoned ticket")
second=$(new_issue "Second abandoned ticket")
first_branch=$(branch_of "$first")
second_branch=$(branch_of "$second")
fixture_ticket_worktree "$first_branch"
fixture_ticket_worktree "$second_branch"
cb issue claim "$first" >/dev/null
cb issue claim "$second" >/dev/null

run_flow milestone abandon "Test milestone" -p FLOW
assert_status 2 "milestone abandon requires explicit confirmation"
assert_eq "$([ -d "$(fixture_milestone_wt)" ] && echo yes)" yes "refusal preserves the milestone worktree"

run_flow milestone abandon "Test milestone" -p FLOW --confirm "direction changed"
assert_status 0 "confirmed milestone abandonment succeeds"
assert_eq "$([ ! -e "$(fixture_milestone_wt)" ] && echo yes)" yes "abandonment removes the milestone worktree"
assert_eq "$(gitf show-ref --verify --quiet refs/heads/milestone/test-milestone || echo missing)" missing "abandonment removes the milestone branch"
assert_eq "$(gitf show-ref --verify --quiet "refs/heads/$first_branch" || echo missing)" missing "abandonment removes the first ticket branch"
assert_eq "$(gitf show-ref --verify --quiet "refs/heads/$second_branch" || echo missing)" missing "abandonment removes the second ticket branch"
assert_board_has "$first" "work discarded: direction changed" "first ticket records why"
assert_board_has "$second" "work discarded: direction changed" "second ticket records why"

fixture_new
foreign="$FIXTURE_ROOT/foreign-milestone-worktree"
gitf worktree add -q "$foreign" milestone/test-milestone

run_flow milestone abandon "Test milestone" -p FLOW --confirm "not ours to remove"
assert_status 0 "foreign milestone worktree abandonment succeeds safely"
assert_eq "$([ -d "$foreign" ] && echo yes)" yes "foreign milestone worktree is retained"
assert_milestone_board_has "Test milestone" "foreign milestone worktree and branch retained" "foreign milestone retention is recorded"

finish
