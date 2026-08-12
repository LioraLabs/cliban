#!/usr/bin/env bash
# later sessions can discover standalone handoffs awaiting reconciliation.
# shellcheck source-path=SCRIPTDIR
# shellcheck source=lib.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

fixture_new
mkdir -p "$FIXTURE_REPO/docs/agents"
printf '# Issue tracker: cliban\n\n**Project key:** FLOW\n' >"$FIXTURE_REPO/docs/agents/issue-tracker.md"
key=$(new_issue_no_milestone "Merged handoff")
cb issue mv "$key" in-review >/dev/null

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
FLOW_OUT=$(cd "$FIXTURE_REPO" && "$ROOT/plugin/hooks/session-start.sh")
assert_out_has "In review:" "session start names the reconciliation candidates"
assert_out_has "$key Merged handoff" "the later session sees the in-review handoff"
assert_out_lacks "binding:" "session start omits the binding parenthetical"
assert_out_lacks "Track work on the board" "session start omits workflow doctrine"

finish
