#!/usr/bin/env bash
# dispatched work stays inspectable and recoverable while it runs.
set -uo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
issue=$ROOT/plugin-flow/skills/complete-issue/SKILL.md
milestone=$ROOT/plugin-flow/skills/complete-milestone/SKILL.md
recovery=$ROOT/plugin-flow/skills/recover-milestone/SKILL.md
failed=0

has() { grep -Fiq -- "$2" "$1" || { printf 'missing %s in %s\n' "$2" "$1" >&2; failed=1; }; }

has "$issue" 'recoverability guarantee'
has "$issue" 'before execution begins'
# commit cadence is implementation judgment, not board ceremony.
# shellcheck disable=SC2016
has "$issue" 'session-start hook surfaces `in-review` candidates'
has "$issue" 'mv <KEY> done --note "merged as <sha>"'

has "$milestone" 'liveness sweep'
has "$milestone" 'running and dead agents'
has "$milestone" 'cliban issue cat <KEY> --section plan'
has "$milestone" 'cliban issue cat <KEY> --section activity'
has "$milestone" 'git log <base>..<ticket-branch>'
has "$milestone" 'git -C <ticket-worktree> status -s'
has "$milestone" 'do not interrupt'
has "$milestone" 'phase and blocker'

for skill in "$milestone" "$recovery"; do
    has "$skill" 'agent ID'
    has "$skill" 'complete-issue'
    has "$skill" 'Resume exception'
    # shellcheck disable=SC2016
    has "$skill" '`send_message`'
done
has "$milestone" 'recover-milestone'
has "$milestone" 'cliban issue release <KEY>'
has "$milestone" 'ticket start <KEY>'
has "$milestone" 'mv <KEY> blocked --note'
has "$milestone" 'independent siblings continue'
has "$milestone" 'dependents wait'
has "$milestone" 'second death'
has "$milestone" 'ask the user'
# shellcheck disable=SC2016
has "$milestone" '`CLI-95` becomes `cli_95`'
# shellcheck disable=SC2016
has "$recovery" '`list_agents`'
# shellcheck disable=SC2016
has "$recovery" '`CLI-95` becomes `cli_95`'
# shellcheck disable=SC2016
has "$recovery" 'load and read `complete-issue`'

exit "$failed"
