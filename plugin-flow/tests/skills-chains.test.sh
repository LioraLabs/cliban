#!/usr/bin/env bash
# a chain staffs one implementer; it never schedules.
set -uo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
scope=$ROOT/plugin-flow/skills/scope-milestone/SKILL.md
complete=$ROOT/plugin-flow/skills/complete-milestone/SKILL.md
failed=0

has() { grep -Fiq -- "$2" "$1" || { printf 'missing %s in %s\n' "$2" "$1" >&2; failed=1; }; }

has "$scope" 'related_to'
has "$scope" 'shared context'
has "$scope" 'user approves'
# the predicted changeset: written at scope time, intersected at wave time.
has "$scope" '## Files'
has "$scope" 'A path/it/will/create.rs'
has "$scope" 'Predict, don'
has "$scope" 'rejects a malformed entry'
has "$complete" 'collisions'
has "$complete" 'predicted to touch one file'
has "$complete" 'slicing problem'
has "$complete" 'never schedule'
has "$complete" 'one implementer'
has "$complete" 'order printed'
has "$complete" 'linear run'
has "$complete" 'current milestone tip'
has "$complete" 'Split a chain only when'
has "$complete" 'worktrees or branches'

exit "$failed"
