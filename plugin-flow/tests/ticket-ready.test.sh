#!/usr/bin/env bash
# CLI-81 — `cliban-flow ticket ready <KEY>`: the handoff, as a board state.
#
# Ready is not a message. `in-review` means "this branch is integrable and its
# tree has been built and tested", and the orchestrator never looks at a branch
# that is not in that state — so the guards below are the whole value of the
# subcommand, and the move is its deliverable rather than its side effect.
#
# Same rule as every suite here: throwaway repo under `mktemp -d`, CLIBAN_DB
# inside it, no path named outside $FIXTURE_ROOT.
# shellcheck source-path=SCRIPTDIR
# shellcheck source=lib.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

# CLI-81: every state shared with `ticket sync` must reach the same guard.
assert_ticket_mutation_guards ready

# ------------------------------------------------------------- the happy path

fixture_new
fixture_milestone_worktree
key=$(new_issue "Synced and green")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
commit_file_at "$(fixture_ticket_wt "$branch")" ticket-side.txt "the ticket's work"
cb issue mv "$key" in-progress >/dev/null
tip=$(gitf rev-parse "$branch")
tsha=$(gitf rev-parse --short "$branch")
msha=$(gitf rev-parse --short milestone/test-milestone)

run_flow ticket ready "$key"
assert_status 0 "a synced branch in a clean tree is ready"
assert_stdout_is "$tip" "the tip, alone, is on stdout"
assert_eq "$(status_of "$key")" "in-review" "the ticket moved to in-review"
assert_timeline_has "$key" "in-progress → in-review" "the move is on the timeline"
assert_timeline_has "$key" "$branch" "the transition note carries the branch"
assert_timeline_has "$key" "$tsha" "the transition note carries the tip SHA"
assert_board_has "$key" "[cliban-flow] ticket ready $key: ready" \
    "the [cliban-flow] line is recorded too — the timeline is a different place"
assert_board_has "$key" "$branch@$tsha" "the board line carries branch and tip"
assert_board_has "$key" "milestone/test-milestone@$msha" \
    "the board line records what the branch was found to contain"
assert_eq "$(gitf rev-parse "$branch")" "$tip" "the branch was not moved"
assert_eq "$(gitt "$branch" status --porcelain)" "" "the worktree was left alone"
assert_stderr_has "orchestrator" "the guidance says who picks it up from here"

# ------------------------------------------------------------ a stale branch
#
# The one refusal the whole protocol rests on. Integration squashes this tree
# unbuilt and untested, which is only safe while it is the tree the implementer
# built — and a branch that does not contain the milestone tip is not that tree.

fixture_new
fixture_milestone_worktree
key=$(new_issue "Behind the milestone")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
commit_file_at "$(fixture_ticket_wt "$branch")" ticket-side.txt "the ticket's work"
commit_file_at "$(fixture_milestone_wt)" milestone-side.txt "another ticket landed"
msha=$(gitf rev-parse --short milestone/test-milestone)
before=$(status_of "$key")

run_flow ticket ready "$key"
assert_status 2 "a branch that does not contain the milestone tip is refused"
assert_stdout_is "" "no tip is printed for a branch that is not ready"
assert_eq "$(status_of "$key")" "$before" "the ticket did not move"
assert_stderr_has "is not an ancestor of $branch" \
    "the refusal names the state it found, not only the remedy"
assert_stderr_has "cliban-flow ticket sync $key" "the refusal directs the caller to sync"
assert_board_has "$key" "[cliban-flow] ticket ready $key: refused" \
    "the refusal is recorded on the board"

# A branch that is merely behind would merge without a conflict, which is why
# "no conflicts" is the wrong gate and ancestry is the right one. Proving that
# here as well as in ticket-status is the point: the two must agree.
gitt "$branch" merge -q --no-edit milestone/test-milestone
assert_eq "$?" "0" "the refused branch would in fact have merged without conflict"

# --------------------------------------------------------------- a dirty tree
#
# What integration reads is the branch. Uncommitted work is not on it, so a
# green suite run over that tree says nothing about what would land.

fixture_new
fixture_milestone_worktree
key=$(new_issue "Work still in flight")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
commit_file_at "$(fixture_ticket_wt "$branch")" ticket-side.txt "the ticket's work"
printf 'not committed\n' >>"$(fixture_ticket_wt "$branch")/ticket-side.txt"
before=$(status_of "$key")

run_flow ticket ready "$key"
assert_status 2 "a dirty ticket worktree is refused"
assert_eq "$(status_of "$key")" "$before" "the ticket did not move"
assert_stderr_has "uncommitted changes" "the refusal says what is in the way"
assert_stderr_has "$(fixture_ticket_wt "$branch")" "the refusal names the tree that is dirty"
assert_eq "$(tail -n1 "$(fixture_ticket_wt "$branch")/ticket-side.txt")" "not committed" \
    "the work in flight was left alone"

# An untracked file is uncommitted too, and is the easier one to leave behind.
fixture_new
fixture_milestone_worktree
key=$(new_issue "Untracked leftovers")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
printf 'scratch\n' >"$(fixture_ticket_wt "$branch")/scratch.txt"

run_flow ticket ready "$key"
assert_status 2 "an untracked file in the ticket worktree is refused"
assert_stderr_has "uncommitted changes" "the refusal says what is in the way"

# ------------------------------------------------------------- idempotency

fixture_new
fixture_milestone_worktree
key=$(new_issue "Declared ready twice")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
commit_file_at "$(fixture_ticket_wt "$branch")" ticket-side.txt "the ticket's work"
tip=$(gitf rev-parse "$branch")
run_flow ticket ready "$key"
assert_status 0 "the first run succeeds"

run_flow ticket ready "$key"
assert_status 0 "a second run succeeds"
assert_stdout_is "$tip" "the second run prints the same tip"
assert_eq "$(status_of "$key")" "in-review" "the ticket is still in-review"
assert_board_has "$key" "[cliban-flow] ticket ready $key: already ready" \
    "the idempotent run is distinguishable on the board from the moving one"

# ------------------------------------------------------- board unavailability
#
# The asymmetry that matters. A failed activity-log append is a warning, because
# the log is an audit trail; a failed move is a refusal, because the move IS
# ready. Reporting success over a board that never received the transition would
# leave a branch nobody will ever look at, with nothing saying so.

fixture_new
fixture_milestone_worktree
key=$(new_issue "Board rejects the move")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
commit_file_at "$(fixture_ticket_wt "$branch")" ticket-side.txt "the ticket's work"
before=$(status_of "$key")
break_board_moves

run_flow ticket ready "$key"
assert_status 2 "a board that rejects the move is a refusal, not a silent success"
assert_stdout_is "" "no tip is printed for a ticket that did not move"
assert_stderr_has "could not move $key to in-review" "the refusal says what failed"
unset FLOW_PATH
assert_eq "$(status_of "$key")" "$before" "the ticket really did not move"

# The audit line, by contrast, degrades to a warning: it is not the deliverable.
fixture_new
fixture_milestone_worktree
key=$(new_issue "Activity log is down")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
commit_file_at "$(fixture_ticket_wt "$branch")" ticket-side.txt "the ticket's work"
break_board_writes

run_flow ticket ready "$key"
assert_status 0 "a failed activity-log append does not undo the move"
assert_stderr_has "could not record" "the failed append is reported"
unset FLOW_PATH
assert_eq "$(status_of "$key")" "in-review" "the ticket moved anyway"

finish
