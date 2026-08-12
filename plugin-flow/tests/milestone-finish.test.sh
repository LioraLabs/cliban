#!/usr/bin/env bash
# `cliban-flow milestone finish <NAME>`: fast-forward onto main.
# shellcheck source-path=SCRIPTDIR
# shellcheck source=lib.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

fixture_new
fixture_milestone_worktree
key=$(new_issue "Integrated ticket")
cb issue mv "$key" "done" >/dev/null
printf 'landed\n' >"$(fixture_milestone_wt)/landed.txt"
git -C "$(fixture_milestone_wt)" add landed.txt
git -C "$(fixture_milestone_wt)" commit -qm "$key: Integrated ticket" -m "Ticket: $key"
tip=$(gitf rev-parse milestone/test-milestone)
run_flow milestone finish "Test milestone" -p FLOW
assert_status 0 "a milestone ahead of main finishes"
assert_stdout_is "$tip" "finish prints the landed SHA alone"
assert_eq "$(gitf rev-parse main)" "$tip" "main fast-forwards to the milestone tip"
assert_eq "$(gitf rev-parse milestone/test-milestone)" "$tip" "the milestone branch stays at its tip"
assert_eq "$(gitf symbolic-ref --short HEAD)" "main" "the primary checkout stays on main"
assert_eq "$(cat "$FIXTURE_REPO/landed.txt")" "landed" "the primary tree is updated with the fast-forward"
assert_milestone_board_has "Test milestone" "[cliban-flow] milestone finish Test milestone: finished" "finish is recorded"

fixture_new
fixture_milestone_worktree
open_key=$(new_issue "Still in flight")
run_flow milestone finish "Test milestone" -p FLOW
assert_status 2 "finish refuses while a milestone issue is open"
assert_stderr_has "$open_key: Still in flight" "the refusal names the open issue"
assert_eq "$(gitf rev-parse main)" "$(gitf rev-parse milestone/test-milestone)" \
    "the open-issue refusal leaves main unchanged"

fixture_new
fixture_milestone_worktree
spoofed=$(new_issue "Mentions discard protocol")
printf 'The operator may later log work discarded: reason.\n' |
    cb issue edit "$spoofed" --section spec --create-section --description-file - >/dev/null
run_flow milestone finish "Test milestone" -p FLOW
assert_status 2 "discard wording outside Activity Log does not settle an issue"
assert_stderr_has "$spoofed: Mentions discard protocol" \
    "the spoofed discard issue remains named as open"

fixture_new
fixture_milestone_worktree
key=$(new_issue "Integrated ticket")
cb issue mv "$key" "done" >/dev/null
printf 'landed\n' >"$(fixture_milestone_wt)/landed.txt"
git -C "$(fixture_milestone_wt)" add landed.txt
git -C "$(fixture_milestone_wt)" commit -qm "$key: Integrated ticket" -m "Ticket: $key"
commit_file_at "$(fixture_milestone_wt)" drift.txt drift
main_tip=$(gitf rev-parse main)
run_flow milestone finish "Test milestone" -p FLOW
assert_status 2 "finish refuses a tip beyond the last verified integration"
assert_stderr_has "differs from the last verified ticket integration" \
    "the refusal names the unverified divergence"
assert_stderr_has "dispatched ticket" "the divergence refusal names the next step"
assert_eq "$(gitf rev-parse main)" "$main_tip" "the divergence refusal leaves main unchanged"

fixture_new
fixture_milestone_worktree
discarded=$(new_issue "Discarded ticket")
cb issue log "$discarded" "work discarded: superseded" >/dev/null
archived=$(new_issue "Archived ticket")
cb issue archive "$archived" >/dev/null
key=$(new_issue "Integrated ticket")
cb issue mv "$key" "done" >/dev/null
printf 'landed\n' >"$(fixture_milestone_wt)/landed.txt"
git -C "$(fixture_milestone_wt)" add landed.txt
git -C "$(fixture_milestone_wt)" commit -qm "$key: Integrated ticket" -m "Ticket: $key"
tip=$(gitf rev-parse milestone/test-milestone)
run_flow milestone finish "Test milestone" -p FLOW
assert_status 0 "archived and explicitly discarded issues are settled"
assert_stdout_is "$tip" "the settled milestone lands its verified tip"

fixture_new
fixture_milestone_worktree
commit_file_at "$(fixture_milestone_wt)" landed.txt landed
gitf checkout -q main
commit_file main.txt drift
main_tip=$(gitf rev-parse main)
milestone_tip=$(gitf rev-parse milestone/test-milestone)
run_flow milestone finish "Test milestone" -p FLOW
assert_status 2 "finish refuses after main drifts"
assert_stderr_has "main is not an ancestor" "the refusal names main drift"
assert_stderr_has "dispatched agent" "the remedy sends the merge to an agent"
assert_stderr_has "main into milestone/test-milestone" "the remedy names the direction"
assert_eq "$(gitf rev-parse main)" "$main_tip" "the refusal leaves main unchanged"
assert_eq "$(gitf rev-parse milestone/test-milestone)" "$milestone_tip" "the refusal leaves the milestone unchanged"
assert_milestone_board_has "Test milestone" "[cliban-flow] milestone finish Test milestone: refused" "the refusal is recorded"

finish
