#!/usr/bin/env bash
# `cliban-flow ticket ready <KEY>`: the handoff, as a board state.
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

# every state shared with `ticket sync` must reach the same guard.
assert_ticket_mutation_guards ready

# readiness requires recoverable plan, ticket work, review, and an
# unfinished board ticket before the handoff can become in-review.

fixture_new
fixture_milestone_worktree
key=$(new_issue "No plan")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
commit_file_at "$(fixture_ticket_wt "$branch")" ticket-side.txt work
cb issue log "$key" "Final review: SPEC ✅; QUALITY pass" >/dev/null
run_flow ticket ready "$key"
assert_status 2 "ready refuses a missing plan"
assert_stderr_has "cliban issue edit $key --section plan" "missing-plan refusal names the repair"

fixture_new
fixture_milestone_worktree
key=$(new_issue "Proportional prose plan")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
commit_file_at "$(fixture_ticket_wt "$branch")" ticket-side.txt work
cb issue edit "$key" --section plan --create-section --description-file - >/dev/null <<'EOF'
Update the one lifecycle document and run its focused prose contract.
EOF
cb issue log "$key" "Final review: SPEC ✅; QUALITY pass" >/dev/null
run_flow ticket ready "$key"
assert_status 0 "ready accepts a non-empty prose plan"
assert_eq "$(status_of "$key")" "in-review" "the prose-planned ticket moved in-review"

fixture_new
fixture_milestone_worktree
key=$(new_issue "Structured plan without steps")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
commit_file_at "$(fixture_ticket_wt "$branch")" ticket-side.txt work
cb issue edit "$key" --section plan --create-section --description-file - >/dev/null <<'EOF'
### Task 1: missing its steps
EOF
cb issue log "$key" "Final review: SPEC ✅; QUALITY pass" >/dev/null
run_flow ticket ready "$key"
assert_status 2 "ready refuses a structured plan without checklist items"
assert_stderr_has "structured plan has no checklist" "step-free structured-plan refusal names the missing evidence"

fixture_new
fixture_milestone_worktree
key=$(new_issue "Unfinished structured plan")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
commit_file_at "$(fixture_ticket_wt "$branch")" ticket-side.txt work
cb issue edit "$key" --section plan --create-section --description-file - >/dev/null <<'EOF'
### Task 1: unfinished

- [ ] **Step 1: still open**
EOF
cb issue log "$key" "Final review: SPEC ✅; QUALITY pass" >/dev/null
run_flow ticket ready "$key"
assert_status 2 "ready refuses an unfinished structured plan"
assert_stderr_has "unfinished checklist" "structured-plan refusal names the missing evidence"

fixture_new
fixture_milestone_worktree
key=$(new_issue "No ticket commits")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
ready_evidence "$key"
run_flow ticket ready "$key"
assert_status 2 "ready refuses zero commits past the milestone"
assert_stderr_has " commit" "zero-commit refusal names the repair"

fixture_new
fixture_milestone_worktree
key=$(new_issue "No review verdict")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
commit_file_at "$(fixture_ticket_wt "$branch")" ticket-side.txt work
cb issue edit "$key" --section plan --create-section --description-file - >/dev/null <<'EOF'
### Task 1: fixture

- [x] **Step 1: exercised**
EOF
run_flow ticket ready "$key"
assert_status 2 "ready refuses a missing review verdict"
assert_stderr_has "cliban issue log $key" "review refusal names the repair"
# Naming only the waiver steered an agent holding a real ACCEPT toward
# recording, permanently, that no review had happened.
assert_stderr_has "review: SPEC ACCEPT; QUALITY pass" \
    "the refusal names the verdict form, not only the waiver"
assert_stderr_has "review waived by orchestrator: <reason>" \
    "the refusal names the waiver form too"

fixture_new
fixture_milestone_worktree
key=$(new_issue "Rejected review")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
commit_file_at "$(fixture_ticket_wt "$branch")" ticket-side.txt work
cb issue edit "$key" --section plan --create-section --description-file - >/dev/null <<'EOF'
small plan
EOF
cb issue log "$key" "review: SPEC REJECT; QUALITY no serious findings" >/dev/null
run_flow ticket ready "$key"
assert_status 2 "ready refuses a rejected review"

cb issue log "$key" "review: SPEC ACCEPT; QUALITY no Critical findings; Important: broken guard remains" >/dev/null
run_flow ticket ready "$key"
assert_status 2 "ready refuses a review with Important findings"

cb issue log "$key" "review: SPEC ACCEPT; QUALITY pass Important: broken guard remains" >/dev/null
run_flow ticket ready "$key"
assert_status 2 "ready refuses contradictory pass text with Important findings"

cb issue log "$key" "review: SPEC ACCEPT; QUALITY pass" >/dev/null
cb issue log "$key" "review: SPEC REJECT; QUALITY Important: regression remains" >/dev/null
run_flow ticket ready "$key"
assert_status 2 "a newer rejection supersedes an older accepted review"

cb issue log "$key" "review: SPEC ACCEPTED; QUALITY pass" >/dev/null
run_flow ticket ready "$key"
assert_status 2 "ready requires an exact accepted Spec verdict"

cb issue log "$key" "review: SPEC ACCEPT; QUALITY passage" >/dev/null
run_flow ticket ready "$key"
assert_status 2 "ready requires an exact passing Quality verdict"

for malformed in \
    'review: UNSPEC ACCEPT; QUALITY pass' \
    'review: SPEC ACCEPT; INEQUALITY pass' \
    'review: NOSPEC ACCEPT; QUALITY pass' \
    'review: SPEC ACCEPT; NONQUALITY pass'; do
    cb issue log "$key" "$malformed" >/dev/null
    run_flow ticket ready "$key"
    assert_status 2 "ready requires exact review field names"
done

fixture_new
key=$(new_issue_no_milestone "Standalone waiver")
branch=$(branch_of "$key")
run_flow ticket start "$key"
commit_file_at "$(fixture_standalone_wt "$branch")" ticket-side.txt work
cb issue edit "$key" --section plan --create-section --description-file - >/dev/null <<'EOF'
small plan
EOF
cb issue log "$key" "review waived by orchestrator: no orchestrator exists" >/dev/null
run_flow ticket ready "$key"
assert_status 2 "standalone ready refuses an orchestrator waiver"
assert_stderr_has "review: SPEC ACCEPT; QUALITY pass" "standalone refusal names a valid repair"
case $FLOW_STDERR in
    *'review waived by orchestrator'*) fail "standalone refusal does not prescribe an impossible waiver" "$FLOW_STDERR" ;;
    *) pass "standalone refusal does not prescribe an impossible waiver" ;;
esac

fixture_new
fixture_milestone_worktree
key=$(new_issue "Malformed review waiver")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
commit_file_at "$(fixture_ticket_wt "$branch")" ticket-side.txt work
cb issue edit "$key" --section plan --create-section --description-file - >/dev/null <<'EOF'
### Task 1: fixture

- [x] **Step 1: exercised**
EOF
cb issue log "$key" "review waived by orchestrator:" >/dev/null
run_flow ticket ready "$key"
assert_status 2 "ready refuses a waiver without a reason"
assert_stderr_has "review waived by orchestrator: <reason>" "waiver refusal prints the exact next record"

fixture_new
fixture_milestone_worktree
key=$(new_issue "Whitespace review waiver")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
commit_file_at "$(fixture_ticket_wt "$branch")" ticket-side.txt work
cb issue edit "$key" --section plan --create-section --description-file - >/dev/null <<'EOF'
### Task 1: fixture

- [x] **Step 1: exercised**
EOF
cb issue log "$key" "review waived by orchestrator:     " >/dev/null
run_flow ticket ready "$key"
assert_status 2 "ready refuses a whitespace-only waiver reason"

fixture_new
fixture_milestone_worktree
key=$(new_issue "Embedded review waiver")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
commit_file_at "$(fixture_ticket_wt "$branch")" ticket-side.txt work
cb issue edit "$key" --section plan --create-section --description-file - >/dev/null <<'EOF'
### Task 1: fixture

- [x] **Step 1: exercised**
EOF
cb issue log "$key" "note — review waived by orchestrator: narrow change" >/dev/null
run_flow ticket ready "$key"
assert_status 2 "ready refuses an embedded waiver phrase"

# A waiver is an authorization, and `cliban issue log` is a tool the ticket's
# own agent holds. Shape alone therefore cannot tell an orchestrator's decision
# from a ticket certifying its own work — authorship can. The identity is
# `agent:<KEY>`, which complete-issue has dispatched work export, and not the
# claim: `ticket start` is routinely run by the orchestrator, so the claim is
# as often the orchestrator's as the agent's.

fixture_new
fixture_milestone_worktree
key=$(new_issue "Self-issued review waiver")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
commit_file_at "$(fixture_ticket_wt "$branch")" ticket-side.txt work
cb issue edit "$key" --section plan --create-section --description-file - >/dev/null <<'EOF'
### Task 1: fixture

- [x] **Step 1: exercised**
EOF
CLIBAN_ACTOR="agent:$key" cb issue log "$key" \
    "review waived by orchestrator: I am the ticket agent" >/dev/null
run_flow ticket ready "$key"
assert_status 2 "ready refuses a waiver the ticket's own agent wrote about its own work"
assert_eq "$(status_of "$key")" "backlog" "the self-waived ticket did not move"
assert_stderr_has "written by agent:$key" \
    "the refusal names who wrote the waiver, not just that one is missing"
assert_stderr_has "review: SPEC ACCEPT; QUALITY pass" \
    "the self-waiver refusal names the verdict form as the way out"

# the claim is not the signal: the orchestrator holds it as often as the agent.
cb issue claim "$key" >/dev/null
cb issue log "$key" "review waived by orchestrator: narrow, well-covered change" >/dev/null
run_flow ticket ready "$key"
assert_status 0 "an orchestrator waiver passes even when the orchestrator holds the claim"
assert_eq "$(status_of "$key")" "in-review" "the orchestrator-waived ticket moved"

fixture_new
fixture_milestone_worktree
key=$(new_issue "Already done")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
commit_file_at "$(fixture_ticket_wt "$branch")" ticket-side.txt work
ready_evidence "$key"
cb issue mv "$key" "done" >/dev/null
run_flow ticket ready "$key"
assert_status 2 "ready refuses a done ticket"
assert_stderr_has "already done" "done refusal names the terminal state"

# ------------------------------------------------------------- the happy path

fixture_new
fixture_milestone_worktree
key=$(new_issue "Synced and green")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
commit_file_at "$(fixture_ticket_wt "$branch")" ticket-side.txt "the ticket's work"
cb issue mv "$key" in-progress >/dev/null
cb issue edit "$key" --section plan --create-section --description-file - >/dev/null <<'EOF'
### Task 1: fixture

- [x] **Step 1: exercised**
EOF
cb issue log "$key" "review waived by orchestrator: narrow, well-covered change" >/dev/null
tip=$(gitf rev-parse "$branch")
tsha=$(gitf rev-parse --short "$branch")
msha=$(gitf rev-parse --short milestone/test-milestone)

run_flow ticket ready "$key"
assert_status 0 "an exact orchestrator waiver makes a synced branch ready"
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
assert_eq "$FLOW_STDERR" "" "ready success adds no ceremony to stderr"

# standalone work uses the same ready handoff without a milestone.
fixture_new
key=$(new_issue_no_milestone "Standalone and green")
branch=$(branch_of "$key")
run_flow ticket start "$key"
commit_file_at "$(fixture_standalone_wt "$branch")" ticket-side.txt work
ready_evidence "$key"
tip=$(gitf rev-parse "$branch")

run_flow ticket ready "$key"
assert_status 0 "a standalone ticket becomes ready"
assert_stdout_is "$tip" "standalone ready prints the immutable tip"
assert_eq "$(status_of "$key")" in-review "standalone ready moves the ticket in-review"
assert_board_has "$key" "$branch@$tip" "standalone ready records the full immutable SHA"

run_flow ticket ready "$key"
assert_status 0 "repeating standalone ready succeeds"
assert_stdout_is "$tip" "repeating standalone ready returns the same immutable tip"
assert_board_has "$key" "already ready ($branch@$tip)" \
    "same-tip standalone ready is retry-safe"

cb issue log "$key" "Final review: SPEC ✅; QUALITY pass before a later commit" >/dev/null
commit_file_at "$(fixture_standalone_wt "$branch")" later.txt late
later=$(gitf rev-parse "$branch")
run_flow ticket ready "$key"
assert_status 2 "a later commit invalidates standalone readiness"
assert_stderr_has "new accepted review evidence" "the refusal names the missing renewed evidence"

cb issue log "$key" "Final review: SPEC ✅; QUALITY pass after $later" >/dev/null
run_flow ticket ready "$key"
assert_status 0 "ready renews a handoff after fresh verification"
assert_stdout_is "$later" "the renewed handoff prints the new immutable tip"
assert_board_has "$key" "ready ($branch@$later)" \
    "a later commit replaces rather than reuses the old readiness evidence"

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
ready_evidence "$key"
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
ready_evidence "$key"
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
ready_evidence "$key"
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
ready_evidence "$key"
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
ready_evidence "$key"
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
ready_evidence "$key"
break_board_writes

run_flow ticket ready "$key"
assert_status 0 "a failed activity-log append does not undo the move"
assert_stderr_has "could not record" "the failed append is reported"
unset FLOW_PATH
assert_eq "$(status_of "$key")" "in-review" "the ticket moved anyway"

finish
