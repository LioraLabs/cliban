#!/usr/bin/env bash
# explicit, recoverable teardown for one ticket.
# shellcheck source-path=SCRIPTDIR
# shellcheck source=lib.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

fixture_new
fixture_milestone_worktree
key=$(new_issue "Abandoned ticket")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
cb issue claim "$key" >/dev/null
cb issue mv "$key" in-progress >/dev/null

run_flow ticket abandon "$key"
assert_status 2 "ticket abandon requires explicit confirmation"
assert_eq "$(status_of "$key")" "in-progress" "refusal preserves board status"
assert_eq "$([ -d "$(fixture_ticket_wt "$branch")" ] && echo yes)" yes "refusal preserves the worktree"

run_flow ticket abandon "$key" --confirm "prototype was rejected"
assert_status 0 "confirmed ticket abandonment succeeds"
assert_eq "$(status_of "$key")" "in-progress" "abandonment preserves board status"
assert_eq "$([ ! -e "$(fixture_ticket_wt "$branch")" ] && echo yes)" yes "abandonment removes the clean worktree"
assert_eq "$(gitf show-ref --verify --quiet "refs/heads/$branch" || echo missing)" missing "abandonment removes the ticket branch"
assert_board_has "$key" "work discarded: prototype was rejected" "abandonment records why"

claim=$(cb issue show "$key" --json | json_get claimed_by)
assert_eq "$claim" "" "abandonment releases the claim"

fixture_new
fixture_milestone_worktree
key=$(new_issue "Dirty abandoned ticket")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
printf 'uncommitted\n' >"$(fixture_ticket_wt "$branch")/scratch.txt"
cb issue claim "$key" >/dev/null

run_flow ticket abandon "$key" --confirm "experiment ended"
assert_status 0 "dirty abandonment still releases the ticket"
assert_eq "$([ -d "$(fixture_ticket_wt "$branch")" ] && echo yes)" yes "dirty worktree is retained"
assert_board_has "$key" "dirty worktree and branch retained" "retention reason is recorded"
claim=$(cb issue show "$key" --json | json_get claimed_by)
assert_eq "$claim" "" "dirty abandonment releases the claim"

fixture_new
fixture_milestone_worktree
key=$(new_issue "Foreign worktree ticket")
branch=$(branch_of "$key")
foreign="$FIXTURE_ROOT/foreign-ticket-worktree"
gitf worktree add -q -b "$branch" "$foreign" refs/heads/milestone/test-milestone

run_flow ticket abandon "$key" --confirm "not ours to remove"
assert_status 0 "foreign worktree abandonment releases the ticket"
assert_eq "$([ -d "$foreign" ] && echo yes)" yes "foreign worktree is retained"
assert_board_has "$key" "foreign worktree and branch retained" "foreign retention reason is recorded"

finish
