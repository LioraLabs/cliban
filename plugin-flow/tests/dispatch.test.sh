#!/usr/bin/env bash
# the dispatcher's own surface: routing, usage, and the exit-code
# contract every subcommand keeps.
# shellcheck source-path=SCRIPTDIR
# shellcheck source=lib.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

fixture_new

run_flow
assert_status 2 "a bare invocation is refused"
assert_out_has "usage: cliban-flow" "a bare invocation prints the usage"

run_flow help
assert_status 0 "help succeeds"
assert_out_has "ticket status" "help lists the subcommands that exist"
# a subcommand that is advertised has to exist, and vice versa; usage
# claiming `ticket start` before it was routed was caught exactly this way.
assert_out_has "milestone start" "help lists milestone start"
assert_out_has "milestone finish" "help lists milestone finish"
# the recovery survey is part of the advertised dispatcher surface.
assert_out_has "milestone status" "help lists milestone status"
assert_out_has "milestone abandon" "help lists milestone abandon"
assert_out_has "ticket start" "help lists ticket start"
assert_out_has "ticket sync" "help lists ticket sync"
assert_out_has "ticket ready" "help lists ticket ready"
assert_out_has "ticket integrate" "help lists ticket integrate"
assert_out_has "ticket abandon" "help lists ticket abandon"

run_flow --help
assert_status 0 "--help is the same as help"

run_flow nonsense thing
assert_status 2 "an unknown command group is refused"
assert_out_has "nonsense" "the refusal names the unknown group"

run_flow ticket
assert_status 2 "a group with no subcommand is refused"

run_flow ticket nonsense
assert_status 2 "an unknown ticket subcommand is refused"
assert_out_has "nonsense" "the refusal names the unknown subcommand"

run_flow ticket status
assert_status 2 "ticket status with no key is refused"

run_flow ticket integrate
assert_status 2 "ticket integrate with no key is refused"

run_flow ticket abandon
assert_status 2 "ticket abandon with no key is refused"

# the milestone group, and the option parsing only it has.
#
# Every case below must refuse before touching anything: the fixture's milestone
# branch is asserted untouched at the end, so a malformed invocation that got as
# far as creating something would show up here rather than in a suite that was
# looking the other way.
milestone_sha=$(gitf rev-parse milestone/test-milestone)

run_flow milestone
assert_status 2 "the milestone group with no subcommand is refused"

run_flow milestone nonsense
assert_status 2 "an unknown milestone subcommand is refused"
assert_out_has "nonsense" "the refusal names the unknown subcommand"

run_flow milestone start
assert_status 2 "milestone start with no name is refused"

run_flow milestone finish
assert_status 2 "milestone finish with no name is refused"

run_flow milestone status
assert_status 2 "milestone status with no name is refused"

run_flow milestone abandon
assert_status 2 "milestone abandon with no name is refused"

run_flow milestone start "Test milestone" "Another milestone" -p FLOW
assert_status 2 "milestone start with two names is refused"

run_flow milestone start "Test milestone" --bogus -p FLOW
assert_status 2 "an unknown option is refused"
assert_out_has "--bogus" "the refusal names the unknown option"

run_flow milestone start "Test milestone" -p
assert_status 2 "a project option with no key is refused"
assert_out_has "project key" "the refusal says what the option needs"

run_flow ticket start
assert_status 2 "ticket start with no key is refused"

run_flow ticket start FLOW-1 FLOW-2
assert_status 2 "ticket start with two keys is refused"

assert_eq "$(gitf rev-parse milestone/test-milestone)" "$milestone_sha" \
    "no malformed invocation moved the milestone branch"
assert_eq "$(gitf worktree list --porcelain | grep -c '^worktree ')" "1" \
    "no malformed invocation created a worktree"

finish
