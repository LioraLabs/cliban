#!/usr/bin/env bash
# CLI-82 — `cliban-flow milestone finish <NAME>`: fast-forward onto main.
# shellcheck source-path=SCRIPTDIR
# shellcheck source=lib.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

fixture_new
fixture_milestone_worktree
commit_file_at "$(fixture_milestone_wt)" landed.txt landed
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
