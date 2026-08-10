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

# A wave is several tickets on one milestone, started one after another. The
# first ticket's worktree lands inside the milestone worktree, and nothing
# obliges a repository to gitignore `.worktrees/` — this fixture deliberately
# does not — so an unqualified dirtiness check would refuse every ticket after
# the first because of a directory this tool created itself.
fixture_new
fixture_milestone_worktree
assert_eq "$(gitf check-ignore .worktrees || true)" "" \
    "the fixture repo does not gitignore .worktrees/, so this case can bite"
first=$(new_issue "First of the wave")
second=$(new_issue "Second of the wave")
run_flow ticket start "$first"
assert_status 0 "the first ticket of a wave starts"

run_flow ticket start "$second"
assert_status 0 "the second ticket of the same wave starts too"
assert_out_lacks "uncommitted" \
    "the first ticket's own worktree is not mistaken for work in flight"

# ------------------------------------------------------------- idempotency

# A wave that is re-run, or an agent respawned into the worktree it already had.
fixture_new
fixture_milestone_worktree
key=$(new_issue "Started twice")
branch=$(branch_of "$key")
run_flow ticket start "$key"
assert_status 0 "the first run succeeds"
sha_before=$(gitf rev-parse "$branch")
worktrees_before=$(gitf worktree list --porcelain | grep -c '^worktree ')

run_flow ticket start "$key"
assert_status 0 "a second run succeeds"
assert_stdout_is "$(fixture_milestone_wt)/.worktrees/$branch" \
    "the second run prints the same worktree path"
assert_eq "$(gitf rev-parse "$branch")" "$sha_before" \
    "the second run did not move the ticket branch onto the newer milestone tip"
assert_eq "$(gitf worktree list --porcelain | grep -c '^worktree ')" "$worktrees_before" \
    "the second run added no second worktree"
assert_stderr_has "already checked out" "the second run says it found what was there"
assert_board_has "$key" "[cliban-flow] ticket start $key: already started" \
    "the idempotent run is distinguishable on the board from the creating one"

# The branch survived its worktree — an agent's tree cleaned up, its work still
# on the branch. Recreating the branch off the milestone tip would discard it.
fixture_new
fixture_milestone_worktree
key=$(new_issue "Branch outlived its worktree")
branch=$(branch_of "$key")
gitf checkout -q -b "$branch" milestone/test-milestone
commit_file ticket-work.txt "work nobody wants discarded"
gitf checkout -q main
tip=$(gitf rev-parse "$branch")

run_flow ticket start "$key"
assert_status 0 "a ticket whose branch exists but is checked out nowhere succeeds"
assert_eq "$(gitf rev-parse "$branch")" "$tip" "the existing ticket branch was not moved"
assert_eq "$(git -C "$(fixture_milestone_wt)/.worktrees/$branch" rev-parse HEAD)" "$tip" \
    "the worktree carries the work the branch already had"
assert_stderr_has "existing branch" "the narration says it is adopting, not creating"
assert_board_has "$key" "[cliban-flow] ticket start $key: adopted" \
    "adopting is distinguishable on the board from creating"

# Registered with git, gone from disk: git will not check the branch out
# anywhere else, so reporting the path would hand back one nothing can use.
fixture_new
fixture_milestone_worktree
key=$(new_issue "Worktree cleaned up under it")
branch=$(branch_of "$key")
run_flow ticket start "$key"
assert_status 0 "the ticket starts"
rm -rf "$(fixture_milestone_wt)/.worktrees/$branch"

run_flow ticket start "$key"
assert_status 2 "a registered ticket worktree whose directory is gone is refused"
assert_stdout_is "" "no dead path is printed as if it were an answer"
assert_stderr_has "no longer exists" "the refusal says what is wrong with it"
assert_stderr_has "worktree prune" "the instruction names the repair it will not do for you"

# ------------------------------------------------------------------ guards

# Mergeability, and therefore a base to branch from, is a question about a
# milestone branch.
fixture_new
fixture_milestone_worktree
key=$(new_issue_no_milestone "Loose ticket")

run_flow ticket start "$key"
assert_status 2 "a ticket on no milestone is refused"
assert_stderr_has "is on no milestone" "the refusal says what is missing"
assert_stderr_has "cliban issue edit $key --milestone" "the instruction is how to fix it"
assert_board_has "$key" "[cliban-flow] ticket start $key: refused" \
    "the refusal is recorded on the board"

# Creating the milestone branch here would answer "what does this integrate
# into" by inventing it.
fixture_new
gitf branch -D milestone/test-milestone >/dev/null
key=$(new_issue "Milestone never started")
branch=$(branch_of "$key")

run_flow ticket start "$key"
assert_status 2 "a missing milestone branch is refused, not created"
assert_stderr_has "cliban-flow milestone start \"Test milestone\" -p FLOW" \
    "the instruction is the subcommand that creates it, runnable as printed"
assert_eq "$(gitf rev-parse --verify --quiet "refs/heads/$branch" || true)" "" \
    "no ticket branch was created either"

# The branch exists but nothing has it checked out: there is no milestone
# worktree to root a ticket worktree under.
fixture_new
key=$(new_issue "Milestone branch with no worktree")

run_flow ticket start "$key"
assert_status 2 "a milestone branch that is checked out nowhere is refused"
assert_stderr_has "no worktree" "the refusal says what is missing"
assert_stderr_has "cliban-flow milestone start \"Test milestone\" -p FLOW" \
    "the instruction is the subcommand that makes one"

# Rooting a ticket worktree under the primary checkout is the layout this whole
# design exists to prevent.
fixture_new
gitf checkout -q milestone/test-milestone
key=$(new_issue "Milestone branch in the primary checkout")
branch=$(branch_of "$key")

run_flow ticket start "$key"
assert_status 2 "a milestone branch checked out in the primary checkout is refused"
assert_stderr_has "primary checkout" "the refusal names what is wrong"
assert_eq "$(ls -d "$FIXTURE_REPO/.worktrees" 2>/dev/null)" "" \
    "nothing was rooted under the primary checkout"
assert_eq "$(gitf rev-parse --verify --quiet "refs/heads/$branch" || true)" "" \
    "no ticket branch was created"

# Uncommitted work in the milestone worktree means the tip this would branch
# from is not the tree that is there.
fixture_new
fixture_milestone_worktree
printf 'work in flight\n' >>"$(fixture_milestone_wt)/base.txt"
key=$(new_issue "Milestone worktree dirty")
branch=$(branch_of "$key")

run_flow ticket start "$key"
assert_status 2 "a dirty milestone worktree is refused"
assert_stderr_has "uncommitted" "the refusal says what is in the way"
assert_stderr_has "$(fixture_milestone_wt)" "the refusal names the tree that is dirty"
assert_eq "$(gitf rev-parse --verify --quiet "refs/heads/$branch" || true)" "" \
    "no ticket branch was created"
assert_eq "$(tail -n1 "$(fixture_milestone_wt)/base.txt")" "work in flight" \
    "the work in flight was left alone"

# The board is stubbed because cliban's own slugifier cannot produce a name
# that climbs out of a directory; the guard exists so that a board which says
# something impossible still cannot make this write outside the milestone root.
fixture_new
fixture_milestone_worktree
key=$(new_issue "Escaping branch name")
forge_branch_name "../../escaped"

run_flow ticket start "$key"
assert_status 2 "a branch name resolving outside the milestone worktree is refused"
assert_stderr_has "outside" "the refusal says the path would leave the milestone worktree"
assert_out_lacks "cliban-flow: creating" \
    "nothing announced that it was about to create anything"
unset FLOW_PATH
assert_eq "$(ls -d "$FIXTURE_ROOT/escaped" 2>/dev/null)" "" \
    "nothing was created outside the milestone worktree"

finish
