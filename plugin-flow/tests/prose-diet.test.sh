#!/usr/bin/env bash
# workflow rules have one owner and a bounded reading cost.
set -uo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
failed=0
lines=$(find "$ROOT/plugin-flow/skills" -type f \( -name SKILL.md -o -path '*/references/*.md' \) -print0 |
    xargs -0 cat | wc -l)
[ "$lines" -le 1000 ] || { echo "workflow prose is $lines lines, cap is 1000" >&2; failed=1; }

lacks() { ! grep -Fq -- "$2" "$1" || { echo "duplicate $2 in $1" >&2; failed=1; }; }
lacks "$ROOT/plugin-flow/skills/complete-milestone/SKILL.md" '**1. Clean integration requires strict ancestry.**'
lacks "$ROOT/plugin-flow/skills/complete-issue/SKILL.md" 'Batching every test for a task'
lacks "$ROOT/plugin-flow/skills/cliban-workflow/SKILL.md" '## Shared Conventions'
# shellcheck disable=SC2016
grep -Fq 'canonical issue labels `bug`, `feature`, `refactor`, and `chore`' \
    "$ROOT/plugin/skills/cliban/SKILL.md" || { echo "canonical-label rule has no owner" >&2; failed=1; }

exit "$failed"
