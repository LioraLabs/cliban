#!/usr/bin/env bash
# CLI-83 — the workflow skills use the dispatcher as their only git protocol.
set -uo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
workflow=$ROOT/plugin-flow/skills/cliban-workflow/SKILL.md
issue=$ROOT/plugin-flow/skills/complete-issue/SKILL.md
milestone=$ROOT/plugin-flow/skills/complete-milestone/SKILL.md
failed=0

has() { grep -Fq -- "$2" "$1" || { printf 'missing %s in %s\n' "$2" "$1" >&2; failed=1; }; }
lacks() { ! grep -Fq -- "$2" "$1" || { printf 'legacy %s in %s\n' "$2" "$1" >&2; failed=1; }; }

has "$workflow" 'plugin-flow/scripts/cliban-flow'
for command in 'milestone start' 'milestone finish' 'ticket start' 'ticket status' 'ticket sync' 'ticket ready' 'ticket integrate'; do
    has "$workflow" "\`$command\`"
done
has "$workflow" 'Dispatched ticket ready for integration'
has "$workflow" 'stop and say so'

has "$issue" 'ticket sync <KEY>'
has "$issue" 'ticket ready <KEY>'
has "$issue" 'resolve the conflicts'
has "$issue" 'resolution diff'

has "$milestone" 'milestone start "<milestone name>"'
has "$milestone" 'ticket start <KEY>'
has "$milestone" 'ticket integrate <KEY>'
has "$milestone" 'milestone finish "<milestone name>"'
has "$milestone" 'own worktree'
has "$milestone" 'strict ancestry'
has "$milestone" 'squash'
lacks "$milestone" 'git checkout'
lacks "$milestone" 'git merge --no-ff'
lacks "$milestone" 'HEAD^2'
lacks "$milestone" '<build the project>'

exit "$failed"
