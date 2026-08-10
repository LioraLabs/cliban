#!/usr/bin/env bash
# CLI-79 — the dispatcher's own surface: routing, usage, and the exit-code
# contract every subcommand keeps.
# shellcheck source=plugin-flow/tests/lib.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

fixture_new

run_flow
assert_status 2 "a bare invocation is refused"
assert_out_has "usage: cliban-flow" "a bare invocation prints the usage"

run_flow help
assert_status 0 "help succeeds"
assert_out_has "ticket status" "help lists the subcommands that exist"

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

finish
