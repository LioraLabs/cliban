#!/usr/bin/env bash
# related tickets are advisory same-implementer chains.
set -uo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
scope=$ROOT/plugin-flow/skills/scope-milestone/SKILL.md
complete=$ROOT/plugin-flow/skills/complete-milestone/SKILL.md
failed=0

has() { grep -Fiq -- "$2" "$1" || { printf 'missing %s in %s\n' "$2" "$1" >&2; failed=1; }; }

has "$scope" 'related_to'
has "$scope" 'shared context'
has "$scope" 'user approves'
has "$complete" 'same implementer'
has "$complete" 'sequentially'
has "$complete" 'current milestone tip'
has "$complete" 'split or extend'
has "$complete" 'worktrees or branches'

exit "$failed"
