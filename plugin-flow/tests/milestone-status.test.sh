#!/usr/bin/env bash
# `cliban-flow milestone status <NAME>`: reconstruct recovery state.
# shellcheck source-path=SCRIPTDIR
# shellcheck source=lib.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

# board state, audit trail, integration, main drift, and a clean
# descendant worktree are visible without changing either authority.
fixture_new
fixture_milestone_worktree
key=$(new_issue "Nearly finished")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
commit_file_at "$(fixture_ticket_wt "$branch")" ticket.txt work
cb issue claim "$key" >/dev/null
cb issue mv "$key" in-progress >/dev/null
cb issue log "$key" "[cliban-flow] ticket sync $key: synced (example)" >/dev/null
before_board=$(cb issue show "$key" --json)
before_refs=$(gitf show-ref)

run_flow milestone status "Test milestone" -p FLOW
assert_status 0 "the survey succeeds"
assert_out_has "milestone: Test milestone" "the milestone is named"
assert_out_has "waves:" "the board-computed waves are printed"
assert_out_has "main-drift: no" "an unchanged main is reported"
assert_out_has "ticket: $key status=in-progress claim=test:cliban-flow" "status and claim are printed"
assert_out_has "integrated: no" "an unintegrated ticket is explicit"
assert_out_has "last-action: [cliban-flow] ticket sync $key: synced (example)" "the last dispatcher action is printed"
assert_out_has "dirty: no" "a clean worktree is reported"
assert_out_has "uncommitted: none" "absence of uncommitted work is explicit"
assert_out_has "merge: no" "absence of an interrupted merge is explicit"
assert_out_has "unmerged: none" "absence of unmerged paths is explicit"
assert_out_has "ahead: 1" "commits ahead of the milestone are counted"
assert_out_has "relation: descendant" "the branch relation is printed"
assert_eq "$(cb issue show "$key" --json)" "$before_board" "the board is byte-for-byte unchanged"
assert_eq "$(gitf show-ref)" "$before_refs" "no ref moved"

# a dirty, divergent worktree left in a conflicting merge reports all
# recovery evidence, including its unmerged paths.
fixture_new
fixture_milestone_worktree
key=$(new_issue "Interrupted sync")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
printf 'ticket\n' >"$(fixture_ticket_wt "$branch")/shared.txt"
gitt "$branch" add shared.txt
gitt "$branch" commit -qm ticket
printf 'milestone\n' >"$(fixture_milestone_wt)/shared.txt"
git -C "$(fixture_milestone_wt)" add shared.txt
git -C "$(fixture_milestone_wt)" commit -qm milestone
gitt "$branch" merge milestone/test-milestone >/dev/null 2>&1 || true
before_board=$(cb issue show "$key" --json)
before_refs=$(gitf show-ref)
before_index=$(gitt "$branch" diff --cached)
before_state=$(gitt "$branch" status --porcelain=v2)
before_merge=$(gitt "$branch" rev-parse MERGE_HEAD)

run_flow milestone status "Test milestone" -p FLOW
assert_status 0 "an interrupted merge is surveyable"
assert_out_has "dirty: yes" "the dirty index is reported"
assert_out_has "uncommitted:" "uncommitted state is printed"
assert_out_has "merge: yes" "the interrupted merge is reported"
assert_out_has "unmerged: shared.txt" "unmerged paths are named"
assert_out_has "relation: diverged" "divergence is distinguished from ancestry"
assert_eq "$(cb issue show "$key" --json)" "$before_board" "the interrupted survey leaves the board unchanged"
assert_eq "$(gitf show-ref)" "$before_refs" "the interrupted survey leaves refs unchanged"
assert_eq "$(gitt "$branch" diff --cached)" "$before_index" "the interrupted survey leaves the index unchanged"
assert_eq "$(gitt "$branch" status --porcelain=v2)" "$before_state" "the interrupted survey leaves worktree state unchanged"
assert_eq "$(gitt "$branch" rev-parse MERGE_HEAD)" "$before_merge" "the interrupted survey leaves MERGE_HEAD unchanged"

# integration, main drift, a missing worktree, and the seven-hour
# silent-agent shape are all called out rather than inferred away.
fixture_new
key=$(new_issue "Silent agent")
branch=$(branch_of "$key")
gitf branch "$branch" milestone/test-milestone
gitf checkout -q "$branch"
commit_file silent.txt work
gitf checkout -q main
gitf checkout -q milestone/test-milestone
gitf merge -q --squash "$branch"
gitf commit -qm "Land silent agent" -m "Ticket: $key"
gitf checkout -q main
commit_file main-drift.txt drift
cb issue claim "$key" >/dev/null
cb issue mv "$key" in-progress >/dev/null

run_flow milestone status "Test milestone" -p FLOW
assert_status 0 "a retained branch without a worktree is surveyable"
assert_out_has "main-drift: yes" "main drift is reported"
assert_out_has "integrated: yes" "the retained branch is recognized as integrated"
assert_out_has "worktree: none" "the missing worktree is explicit"
assert_out_has "silent-agent: yes" "commits plus an unticked plan is called out"

# a registered worktree whose directory vanished is missing, not clean.
fixture_new
fixture_milestone_worktree
key=$(new_issue "Vanished worktree")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
missing=$(fixture_ticket_wt "$branch")
rm -rf -- "$missing"

run_flow milestone status "Test milestone" -p FLOW
assert_status 0 "a vanished registered worktree is surveyable"
assert_out_has "worktree: $missing (missing)" "a vanished registered worktree is explicit"
assert_out_lacks "dirty: no" "a vanished worktree is not reported as clean"
assert_out_has "relation: level" "a level branch is explicit"
assert_out_has "last-action: none" "absence of a dispatcher action is explicit"

# missing refs remain diagnosable rather than becoming git errors.
fixture_new
key=$(new_issue "Missing refs")
gitf branch -D milestone/test-milestone >/dev/null

run_flow milestone status "Test milestone" -p FLOW
assert_status 0 "missing milestone and ticket branches are surveyable"
assert_out_has "branch: missing" "a missing milestone branch is explicit"
assert_out_has "main-drift: unknown" "drift without a milestone branch is unknown"
assert_out_has "worktree: none" "an absent ticket worktree is explicit"
assert_out_has "relation: missing" "a missing ticket branch has a missing relation"

# an older ticket branch and a ticked plan suppress the silent warning.
fixture_new
fixture_milestone_worktree
key=$(new_issue "Known progress")
branch=$(branch_of "$key")
gitf branch "$branch" milestone/test-milestone
commit_file_at "$(fixture_milestone_wt)" milestone-later.txt later
cb issue claim "$key" >/dev/null
cb issue mv "$key" in-progress >/dev/null
printf '%s\n' '### Task 1: Progress' '' '- [x] **Step 1:** committed' |
    cb issue edit "$key" --section plan --create-section --description-file - >/dev/null

run_flow milestone status "Test milestone" -p FLOW
assert_status 0 "an ancestor ticket with board progress is surveyable"
assert_out_has "relation: ancestor" "an ancestor ticket branch is explicit"
assert_out_has "silent-agent: no" "a ticked plan suppresses the silent-agent warning"

# integration trailers match a complete key, not a key prefix.
fixture_new
fixture_milestone_worktree
key=$(new_issue "Prefix ticket")
git -C "$(fixture_milestone_wt)" commit --allow-empty -qm prefix -m "Ticket: ${key}0"

run_flow milestone status "Test milestone" -p FLOW
assert_status 0 "a longer ticket trailer is surveyable"
assert_out_has "integrated: no" "a ticket key prefix is not falsely integrated"

finish
