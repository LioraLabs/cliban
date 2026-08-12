#!/usr/bin/env bash
# `cliban-flow ticket sync <KEY>`: the milestone tip, merged into the
# ticket branch, in the ticket's own worktree.
#
# Same rule as every suite here: throwaway repo under `mktemp -d`, CLIBAN_DB
# inside it, no path named outside $FIXTURE_ROOT. This one runs a real `git
# merge`, which is the reason that rule is not negotiable.
# shellcheck source-path=SCRIPTDIR
# shellcheck source=lib.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

# every state shared with `ticket ready` must reach the same guard.
assert_ticket_mutation_guards sync

# ------------------------------------------------------------- a clean merge

fixture_new
fixture_milestone_worktree
key=$(new_issue "Behind, and conflict-free")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
commit_file_at "$(fixture_ticket_wt "$branch")" ticket-side.txt "the ticket's work"
commit_file_at "$(fixture_milestone_wt)" milestone-side.txt "another ticket landed"
mtip=$(gitf rev-parse milestone/test-milestone)
msha=$(gitf rev-parse --short milestone/test-milestone)
tsha=$(gitf rev-parse --short "$branch")

run_flow ticket sync "$key"
assert_status 0 "a behind, conflict-free branch syncs"
assert_stdout_is "$(gitf rev-parse "$branch")" \
    "the resulting tip, alone, is on stdout"
assert_eq "$(gitf rev-list --parents -n1 "$branch" | wc -w)" "3" \
    "the sync made a merge commit, not a rebase or a reset"
gitf merge-base --is-ancestor "$mtip" "refs/heads/$branch"
assert_eq "$?" "0" "the ticket branch now contains the milestone tip"
assert_eq "$(gitf rev-parse milestone/test-milestone)" "$mtip" \
    "the milestone branch was not moved"
assert_eq "$(cat "$(fixture_ticket_wt "$branch")/ticket-side.txt")" "the ticket's work" \
    "the ticket's own work survived the merge"
assert_eq "$(cat "$(fixture_ticket_wt "$branch")/milestone-side.txt")" "another ticket landed" \
    "the milestone's work arrived in the ticket worktree"
assert_eq "$(gitt "$branch" status --porcelain)" "" "the ticket worktree was left clean"
assert_eq "$(gitf rev-parse --abbrev-ref HEAD)" "main" "the primary checkout is untouched"
assert_eq "$FLOW_STDERR" "" "successful sync adds no ceremony to stderr"
assert_out_lacks "tests passed" "this tool does not run anything it could report on"
assert_board_has "$key" "[cliban-flow] ticket sync $key: merged" \
    "the sync is recorded on the board"
assert_board_has "$key" "milestone/test-milestone@$msha into $branch@$tsha" \
    "the board line records which tip went into which branch, and where each was"
assert_board_has "$key" "clean" "the board line records that nothing had to be resolved"

# ---------------------------------------------------------- a fast-forward
#
# A ticket branch with no commits of its own is strictly behind. Merging leaves
# no merge commit, and the branch lands exactly on the milestone tip.

fixture_new
fixture_milestone_worktree
key=$(new_issue "No work of its own")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
commit_file_at "$(fixture_milestone_wt)" milestone-side.txt "another ticket landed"
mtip=$(gitf rev-parse milestone/test-milestone)

run_flow ticket sync "$key"
assert_status 0 "a branch with no work of its own syncs"
assert_stdout_is "$mtip" "it lands exactly on the milestone tip"
assert_eq "$(gitf rev-list --parents -n1 "$branch" | wc -w)" "2" \
    "no empty merge commit was manufactured"
assert_board_has "$key" "fast-forward" \
    "the board line distinguishes a fast-forward from a merge commit"

# --------------------------------------------------------- already up to date
#
# The state a second run finds, and the state a ticket started this wave is
# already in. It must not manufacture a merge commit to have something to say.

fixture_new
fixture_milestone_worktree
key=$(new_issue "Already carrying the tip")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
commit_file_at "$(fixture_ticket_wt "$branch")" ticket-side.txt "the ticket's work"
before=$(gitf rev-parse "$branch")

run_flow ticket sync "$key"
assert_status 0 "a branch already containing the milestone tip succeeds"
assert_stdout_is "$before" "stdout is the tip it was already on"
assert_eq "$(gitf rev-parse "$branch")" "$before" "the branch was not moved"
assert_eq "$FLOW_STDERR" "" "up-to-date success adds no ceremony to stderr"
assert_board_has "$key" "[cliban-flow] ticket sync $key: already up to date" \
    "the no-op is distinguishable on the board from a merge"

# A second sync straight after a real one is the same state, reached the other
# way. The pair is what pins idempotency; either case alone does not.
fixture_new
fixture_milestone_worktree
key=$(new_issue "Synced twice")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
commit_file_at "$(fixture_ticket_wt "$branch")" ticket-side.txt "the ticket's work"
commit_file_at "$(fixture_milestone_wt)" milestone-side.txt "another ticket landed"
run_flow ticket sync "$key"
assert_status 0 "the first sync succeeds"
after_first=$(gitf rev-parse "$branch")

run_flow ticket sync "$key"
assert_status 0 "the second sync succeeds"
assert_eq "$(gitf rev-parse "$branch")" "$after_first" \
    "the second sync did not stack a second merge commit"

# ---------------------------------------------------------------- a conflict
#
# The one outcome this subcommand is really for. It is exit 1, not a refusal:
# nothing was misused and no guard failed — a conflict is what a correct merge
# of two correct branches does, and the implementer is the one who resolves it.

fixture_new
fixture_milestone_worktree
key=$(new_issue "Diverged on one file")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
commit_file_at "$(fixture_ticket_wt "$branch")" shared.txt "the ticket's line"
commit_file_at "$(fixture_milestone_wt)" shared.txt "the milestone's line"
before=$(gitf rev-parse "$branch")
msha=$(gitf rev-parse --short milestone/test-milestone)

run_flow ticket sync "$key"
assert_status 1 "a conflicting sync exits 1 — a real outcome, not a refusal"
assert_stdout_is "shared.txt" "the unmerged paths, alone, are on stdout"
assert_eq "$(gitt "$branch" rev-parse --verify --quiet MERGE_HEAD >/dev/null && echo yes)" "yes" \
    "the merge is left in progress for the implementer to resolve"
assert_eq "$(gitf rev-parse "$branch")" "$before" "the branch was not moved"
assert_eq "$(grep -c '^<<<<<<<' "$(fixture_ticket_wt "$branch")/shared.txt")" "1" \
    "the conflict markers are in the tree, where they get resolved"
# The three things the spec requires this refusal to say.
assert_stderr_has "The agent implementing $key resolves this" \
    "the instruction says the implementer owns the resolution"
assert_stderr_has "The orchestrator will not do it" \
    "the instruction says the orchestrator will not do it"
assert_stderr_has "cliban issue log $key" \
    "the instruction says the resolution has to be logged to the ticket"
assert_stderr_has "$(fixture_ticket_wt "$branch")" \
    "the instruction names the worktree the merge is in"
[ "$(printf '%s\n' "$FLOW_STDERR" | wc -l)" -le 6 ] || \
    fail "conflict guidance is at most six lines" "$FLOW_STDERR"
assert_board_has "$key" "[cliban-flow] ticket sync $key: conflicted" \
    "the conflict is recorded on the board"
assert_board_has "$key" "milestone/test-milestone@$msha into $branch" \
    "the conflicted line names both sides, like the clean one"
assert_board_has "$key" "unmerged: shared.txt" \
    "the board line records which paths had to be resolved"

# More than one conflicted path: stdout is a list, one per line, so a caller can
# read it with `mapfile`.
fixture_new
fixture_milestone_worktree
key=$(new_issue "Diverged on two files")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
commit_file_at "$(fixture_ticket_wt "$branch")" a.txt "the ticket's a"
commit_file_at "$(fixture_ticket_wt "$branch")" b.txt "the ticket's b"
commit_file_at "$(fixture_milestone_wt)" a.txt "the milestone's a"
commit_file_at "$(fixture_milestone_wt)" b.txt "the milestone's b"

run_flow ticket sync "$key"
assert_status 1 "two conflicting paths still exit 1"
assert_stdout_is "a.txt
b.txt" "both unmerged paths are on stdout, one per line"
assert_board_has "$key" "unmerged: a.txt b.txt" "the board line records both"

# The resolution, committed by the implementer, is what makes the next sync a
# no-op. This is the round trip the protocol depends on.
printf 'resolved\n' >"$(fixture_ticket_wt "$branch")/a.txt"
printf 'resolved\n' >"$(fixture_ticket_wt "$branch")/b.txt"
gitt "$branch" add a.txt b.txt
gitt "$branch" commit -qm "resolve"

run_flow ticket sync "$key"
assert_status 0 "after the implementer commits the resolution, the branch is in sync"
assert_board_has "$key" "already up to date" "the round trip ends on the board"

finish
