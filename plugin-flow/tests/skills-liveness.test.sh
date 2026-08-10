#!/usr/bin/env bash
# CLI-78 — dispatched work stays inspectable and recoverable while it runs.
set -uo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
issue=$ROOT/plugin-flow/skills/complete-issue/SKILL.md
milestone=$ROOT/plugin-flow/skills/complete-milestone/SKILL.md
failed=0

has() { grep -Fiq -- "$2" "$1" || { printf 'missing %s in %s\n' "$2" "$1" >&2; failed=1; }; }

has "$issue" 'recoverability guarantee'
has "$issue" 'before execution begins'
has "$issue" 'commit new files early'

has "$milestone" 'liveness sweep'
has "$milestone" 'running and dead agents'
has "$milestone" 'cliban issue cat <KEY> --section plan'
has "$milestone" 'cliban issue cat <KEY> --section activity'
has "$milestone" 'git log <base>..<ticket-branch>'
has "$milestone" 'git -C <ticket-worktree> status -s'
has "$milestone" 'do not interrupt'
has "$milestone" 'phase and blocker'

exit "$failed"
