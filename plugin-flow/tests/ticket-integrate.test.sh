#!/usr/bin/env bash
# CLI-82 — `cliban-flow ticket integrate <KEY>`: one guarded squash commit.
# shellcheck source-path=SCRIPTDIR
# shellcheck source=lib.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

ready_ticket() {
    key=$(new_issue "$1")
    branch=$(branch_of "$key")
    fixture_ticket_worktree "$branch"
    commit_file_at "$(fixture_ticket_wt "$branch")" ticket.txt "$1"
    cb issue mv "$key" in-review >/dev/null
}

fixture_new
fixture_milestone_worktree
ready_ticket "Dry run ticket"
mtip=$(gitf rev-parse milestone/test-milestone)
ttip=$(gitf rev-parse "$branch")
activity_before=$(cb issue cat "$key" --section activity 2>&1 || true)
run_flow ticket integrate "$key" --dry-run
assert_status 0 "dry-run accepts a ready, synced ticket"
assert_stdout_is "mergeable: milestone/test-milestone@$mtip is an ancestor of $branch@$ttip
squash: $key: Dry run ticket
milestone: milestone/test-milestone@$mtip
ticket: $branch@$ttip" "dry-run prints the complete merge readout"
assert_eq "$(gitf rev-parse milestone/test-milestone)" "$mtip" "dry-run does not move the milestone"
assert_eq "$(status_of "$key")" "in-review" "dry-run does not move the board"
assert_eq "$(cb issue cat "$key" --section activity 2>&1 || true)" "$activity_before" "dry-run writes no activity"
assert_eq "$(gitf worktree list --porcelain | grep -c "$(fixture_ticket_wt "$branch")")" "1" "dry-run keeps the worktree"

fixture_new
fixture_milestone_worktree
ready_ticket "Land this ticket"
commit_file_at "$(fixture_ticket_wt "$branch")" second.txt "second change"
old_mtip=$(gitf rev-parse milestone/test-milestone)
ttip=$(gitf rev-parse "$branch")
run_flow ticket integrate "$key"
assert_status 0 "a ready, synced ticket integrates"
new_mtip=$(gitf rev-parse milestone/test-milestone)
assert_stdout_is "$new_mtip" "integration prints the squash SHA alone"
assert_eq "$(gitf rev-parse "$branch")" "$ttip" "the ticket branch is retained"
assert_eq "$(status_of "$key")" "done" "integration moves the ticket to done"
assert_eq "$(gitf worktree list --porcelain | grep -c "$(fixture_ticket_wt "$branch")")" "0" "integration removes the worktree only"
assert_eq "$(gitf log -1 --format=%s milestone/test-milestone)" "$key: Land this ticket" "the squash subject names the ticket"
message=$(gitf log -1 --format=%B milestone/test-milestone)
if printf '%s' "$message" | grep -qF -- "Ticket: $key"; then pass "the commit carries the ticket trailer"; else fail "the commit carries the ticket trailer" "commit message: $message"; fi
if printf '%s' "$message" | grep -qF -- "ticket.txt: Land this ticket" && printf '%s' "$message" | grep -qF -- "second.txt: second change"; then pass "the body lists every discarded commit subject"; else fail "the body lists every discarded commit subject" "commit message: $message"; fi
assert_board_has "$key" "[cliban-flow] ticket integrate $key: integrated" "integration is recorded"
assert_timeline_has "$key" "$new_mtip" "the done transition carries the squash SHA"
assert_eq "$(gitf diff "$old_mtip" "$new_mtip" -- ticket.txt second.txt | grep -c '^+[^+]')" "2" "the squash carries the ticket tree"

fixture_new
fixture_milestone_worktree
key=$(new_issue "Not reviewed")
branch=$(branch_of "$key")
fixture_ticket_worktree "$branch"
commit_file_at "$(fixture_ticket_wt "$branch")" ticket.txt work
before=$(gitf rev-parse milestone/test-milestone)
run_flow ticket integrate "$key"
assert_status 2 "integration refuses a ticket not in-review"
assert_stderr_has "$key is backlog" "the refusal names the actual status"
assert_eq "$(gitf rev-parse milestone/test-milestone)" "$before" "the refusal changes no ref"

fixture_new
fixture_milestone_worktree
ready_ticket "Stale ticket"
commit_file_at "$(fixture_milestone_wt)" landed.txt landed
run_flow ticket integrate "$key"
assert_status 2 "integration refuses a stale ticket"
assert_stderr_has "agent implementing $key" "the refusal names the responsible agent"
assert_stderr_has "orchestrator must not" "the refusal forbids orchestrator resolution"

fixture_new
fixture_milestone_worktree
ready_ticket "Main drift"
gitf checkout -q main
commit_file main.txt drift
run_flow ticket integrate "$key"
assert_status 2 "integration surfaces main drift"
assert_stderr_has "main is not an ancestor" "the refusal names main drift"
assert_stderr_has "dispatched agent" "the remedy assigns the merge to an agent"

fixture_new
fixture_milestone_worktree
ready_ticket "Late commit"
gitf config core.hooksPath "$FIXTURE_REPO/.git/hooks"
cat >"$FIXTURE_REPO/.git/hooks/post-commit" <<EOF
#!/usr/bin/env bash
if [ ! -e "$FIXTURE_ROOT/late-fired" ]; then
    touch "$FIXTURE_ROOT/late-fired"
    unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE
    git -C "$(fixture_ticket_wt "$branch")" commit --allow-empty -m 'late ticket commit' >/dev/null
fi
EOF
chmod +x "$FIXTURE_REPO/.git/hooks/post-commit"
run_flow ticket integrate "$key"
assert_status 2 "integration fails loudly when the ticket tip moves"
assert_stderr_has "moved after integration started" "the refusal names the race"
assert_eq "$(status_of "$key")" "in-review" "late detection does not complete the ticket"
assert_eq "$(gitf worktree list --porcelain | grep -c "$(fixture_ticket_wt "$branch")")" "1" "late detection retains the worktree"

finish
