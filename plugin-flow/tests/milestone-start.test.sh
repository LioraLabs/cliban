#!/usr/bin/env bash
# CLI-80, CLI-100 — `cliban-flow milestone start <NAME>`: the milestone's git layout.
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
assert_eq "$FLOW_STDERR" "" "success adds no ceremony to stderr"
assert_milestone_board_has "Test milestone" \
    "[cliban-flow] milestone start Test milestone: created" \
    "the milestone's own activity log records the line"

# ------------------------------------------------------------- idempotency

# The orchestrator re-runs this after a crash without knowing how far the last
# run got, so a second run must be a report, not a rebuild.
fixture_new
gitf branch -D milestone/test-milestone >/dev/null
run_flow milestone start "Test milestone" -p FLOW
assert_status 0 "the first run succeeds"
sha_before=$(gitf rev-parse milestone/test-milestone)
worktrees_before=$(gitf worktree list --porcelain | grep -c '^worktree ')

run_flow milestone start "Test milestone" -p FLOW
assert_status 0 "a second run succeeds"
assert_stdout_is "$(milestone_wt)" "the second run prints the same worktree path"
assert_eq "$(gitf rev-parse milestone/test-milestone)" "$sha_before" \
    "the second run did not move the milestone branch"
assert_eq "$(gitf worktree list --porcelain | grep -c '^worktree ')" "$worktrees_before" \
    "the second run added no second worktree"
assert_eq "$FLOW_STDERR" "" "idempotent success adds no ceremony to stderr"
assert_milestone_board_has "Test milestone" \
    "[cliban-flow] milestone start Test milestone: already started" \
    "the idempotent run is distinguishable on the board from the creating one"

# The branch outlived its worktree — a pruned or manually removed one. The
# existing branch is adopted; recreating it off main would silently discard
# every ticket already integrated into it.
fixture_new
gitf checkout -q milestone/test-milestone
commit_file integrated.txt "a ticket already landed here"
gitf checkout -q main
tip=$(gitf rev-parse milestone/test-milestone)

run_flow milestone start "Test milestone" -p FLOW
assert_status 0 "a milestone whose branch exists but is checked out nowhere succeeds"
assert_eq "$(gitf rev-parse milestone/test-milestone)" "$tip" \
    "the existing milestone branch was not moved back to main"
assert_eq "$(git -C "$(milestone_wt)" rev-parse HEAD)" "$tip" \
    "the worktree carries the work the branch already had"
assert_eq "$FLOW_STDERR" "" "adoption success adds no ceremony to stderr"
assert_eq "$(gitf rev-parse --abbrev-ref HEAD)" "main" \
    "adopting leaves the primary checkout on what it was on"
assert_eq "$(gitf status --porcelain)" "" "adopting leaves the primary checkout clean"
assert_milestone_board_has "Test milestone" \
    "[cliban-flow] milestone start Test milestone: adopted" \
    "adopting is distinguishable on the board from creating"

# ------------------------------------------------------------------ guards

# The incident this whole layout exists to prevent: the milestone branch in the
# tree the user works and runs from. Reporting it as "already started" would
# bless exactly that state.
fixture_new
gitf checkout -q milestone/test-milestone

run_flow milestone start "Test milestone" -p FLOW
assert_status 2 "a milestone branch checked out in the primary checkout is refused"
assert_stderr_has "primary checkout" "the refusal names what is wrong"
assert_stderr_has "git -C \"$FIXTURE_REPO\" checkout main" \
    "the instruction is the command that fixes it, quoted so it survives a path with spaces"
assert_eq "$(printf '%s\n' "$FLOW_STDERR" | wc -l)" "3" \
    "a refusal is exactly what, why, and next-command"
assert_stdout_is "" "a refusal puts nothing on stdout"
assert_eq "$(ls -d "$(milestone_wt)" 2>/dev/null)" "" "no worktree was created"
assert_milestone_board_has "Test milestone" \
    "[cliban-flow] milestone start Test milestone: refused" \
    "the refusal is recorded on the board"

# The state a crashed run plus a cleanup leaves: git still holds the
# registration, so it will not check the branch out anywhere else, and a path
# that no longer exists is not something a caller can cd into.
fixture_new
gitf branch -D milestone/test-milestone >/dev/null
run_flow milestone start "Test milestone" -p FLOW
assert_status 0 "the milestone starts"
rm -rf "$(milestone_wt)"

run_flow milestone start "Test milestone" -p FLOW
assert_status 2 "a registered worktree whose directory is gone is refused, not reported"
assert_stdout_is "" "no dead path is printed as if it were an answer"
assert_stderr_has "no longer exists" "the refusal says what is wrong with it"
assert_stderr_has "worktree prune" "the instruction names the repair it will not do for you"
assert_eq "$(gitf worktree list --porcelain | grep -c '^worktree ')" "2" \
    "the stale registration was left for the operator to clear"

# Slugifying is lossy, and cliban accepts both of these names in one project.
fixture_new
cb milestone add "Test-Milestone!" --project FLOW >/dev/null
gitf branch -D milestone/test-milestone >/dev/null

run_flow milestone start "Test-Milestone!" -p FLOW
assert_status 2 "two milestones deriving one branch name are refused"
assert_stderr_has "both name the branch milestone/test-milestone" \
    "the refusal names the collision and the branch they collide on"
assert_eq "$(gitf rev-parse --verify --quiet refs/heads/milestone/test-milestone || true)" "" \
    "neither milestone got the branch"
assert_milestone_board_has "Test-Milestone!" \
    "[cliban-flow] milestone start Test-Milestone!: refused" \
    "a refusal on a milestone that did resolve lands on its own log"

# A typo would otherwise create a real branch and a real worktree for a
# milestone nobody is tracking, and its board line would go nowhere.
fixture_new
run_flow milestone start "No such milestone" -p FLOW
assert_status 2 "a milestone that is not on the board is refused"
assert_stderr_has "No such milestone" "the refusal names the milestone it could not find"
assert_eq "$(gitf rev-parse --verify --quiet refs/heads/milestone/no-such-milestone || true)" "" \
    "no branch was created for it"
assert_eq "$(ls -d "$FIXTURE_ROOT"/repo-no-such-milestone 2>/dev/null)" "" \
    "no worktree was created for it"

# Milestone names are unique per project, not globally, so an unscoped lookup
# would be a guess.
fixture_new
gitf checkout -q milestone/test-milestone
commit_file integrated.txt "a ticket already landed here"
gitf checkout -q main
tip=$(gitf rev-parse milestone/test-milestone)

run_flow milestone start "Test milestone"
assert_status 2 "no project scope is refused"
assert_stderr_has "no project scope" "the refusal says what is missing"
assert_stderr_has "-p <KEY>" "the instruction shows how to supply it"
# Compared against a sha read before the run: comparing the branch to itself
# afterwards agrees by construction and would only ever catch a deletion.
assert_eq "$(gitf rev-parse milestone/test-milestone)" "$tip" \
    "the milestone branch was not moved"

# $CLIBAN_PROJECT is the same scope by another route.
CLIBAN_PROJECT=FLOW run_flow milestone start "Test milestone"
assert_status 0 "the scope can come from the environment instead of -p"
assert_stdout_is "$(milestone_wt)" "and produces the same worktree"

# --project=KEY is the long option's other spelling.
fixture_new
gitf branch -D milestone/test-milestone >/dev/null
run_flow milestone start "Test milestone" --project=FLOW
assert_status 0 "--project=KEY is accepted"
assert_stdout_is "$(milestone_wt)" "and names the same worktree"

# A name that slugifies to nothing has no branch it could mean.
fixture_new
cb milestone add "!!!" --project FLOW >/dev/null
run_flow milestone start "!!!" -p FLOW
assert_status 2 "a milestone whose name slugifies to nothing is refused"
assert_stderr_has "no usable branch name" "the refusal says why"
assert_eq "$(gitf rev-parse --verify --quiet refs/heads/milestone/ || true)" "" \
    "no branch was created from the empty slug"
assert_milestone_board_has "!!!" "[cliban-flow] milestone start !!!: refused" \
    "that refusal lands on the milestone's own log too"

# The derived path is taken by a worktree of this same repository that is simply
# not on the milestone branch — detached after a bisect, or left on another
# branch. "Move it aside" would break git's registration; the repair is to check
# the branch out where it belongs.
fixture_new
gitf worktree add -q --detach "$(fixture_milestone_wt)" main

run_flow milestone start "Test milestone" -p FLOW
assert_status 2 "a registered worktree at the derived path that is not on the branch is refused"
assert_stderr_has "detached" "the refusal names the state it actually found"
assert_stderr_has "git -C \"$(fixture_milestone_wt)\" checkout milestone/test-milestone" \
    "the instruction is the repair for that state, not advice that would break it"
assert_out_lacks "move it aside" "it does not suggest moving a live worktree"

# An unreachable board must never turn a completed action into a failure.
fixture_new
gitf branch -D milestone/test-milestone >/dev/null
break_milestone_board_writes
run_flow milestone start "Test milestone" -p FLOW
assert_status 0 "a board that will not take the line does not fail the action"
assert_stdout_is "$(milestone_wt)" "the worktree path is still the answer"
assert_stderr_has "the board is out of date" "the lost line is reported as a warning"
unset FLOW_PATH
assert_eq "$(gitf rev-parse --abbrev-ref HEAD)" "main" "the primary checkout is still on main"

# The integration branch starts at main by definition; inventing a base would
# be choosing what the milestone integrates into.
fixture_new
gitf branch -D milestone/test-milestone >/dev/null
gitf checkout -q -b trunk main
gitf branch -D main >/dev/null

run_flow milestone start "Test milestone" -p FLOW
assert_status 2 "a repository with no main branch is refused"
assert_stderr_has "no main branch" "the refusal says why it cannot proceed"
assert_stderr_has 'next: cliban-flow milestone start Test\ milestone -p FLOW' \
    "a refusal without a specialized repair prints the retry command"
assert_eq "$(gitf rev-parse --verify --quiet refs/heads/milestone/test-milestone || true)" "" \
    "no milestone branch was created off something else"

# Whatever is sitting at that path was put there by someone. Refusing beats
# reusing it and beats removing it.
fixture_new
gitf branch -D milestone/test-milestone >/dev/null
mkdir -p "$(milestone_wt)"
printf 'not a worktree\n' >"$(milestone_wt)/someones-file"

run_flow milestone start "Test milestone" -p FLOW
assert_status 2 "an occupied worktree path is refused"
# Specific to this guard rather than to git's own "already exists": letting the
# worktree add fail and reporting that would also exit 2, and the difference is
# whether the refusal happened before anything was attempted.
assert_stderr_has "already exists and is not a worktree for milestone/test-milestone" \
    "the refusal is this guard's, made before anything was attempted"
assert_stderr_has "Refusing rather than" "the refusal says why it will not clean up for you"
assert_eq "$(cat "$(milestone_wt)/someones-file")" "not a worktree" \
    "nothing at that path was removed or overwritten"
assert_eq "$(gitf rev-parse --verify --quiet refs/heads/milestone/test-milestone || true)" "" \
    "no branch was created either"

# A dangling symlink is invisible to -e, so the same guard has to test -L too;
# without that it falls through, announces that it is creating, and reports
# git's error instead of its own.
fixture_new
gitf branch -D milestone/test-milestone >/dev/null
ln -s "$FIXTURE_ROOT/gone" "$(milestone_wt)"

run_flow milestone start "Test milestone" -p FLOW
assert_status 2 "a dangling symlink at the worktree path is refused"
assert_stderr_has "already exists and is not a worktree for milestone/test-milestone" \
    "the refusal is this guard's, not git's"
assert_out_lacks "creating milestone/test-milestone" \
    "nothing announced that it was about to create anything"

finish
