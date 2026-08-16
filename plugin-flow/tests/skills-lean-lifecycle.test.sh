#!/usr/bin/env bash
set -uo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
issue=$ROOT/plugin-flow/skills/complete-issue/SKILL.md
workflow=$ROOT/plugin-flow/skills/cliban-workflow/SKILL.md
milestone=$ROOT/plugin-flow/skills/complete-milestone/SKILL.md
setup=$ROOT/plugin/skills/setup-cliban/SKILL.md
adapter=$ROOT/docs/agents/issue-tracker.md
review=$ROOT/plugin-flow/skills/complete-issue/references/review.md
verify=$ROOT/plugin-flow/skills/complete-issue/references/verification.md
failed=0

has() { grep -Fiq -- "$2" "$1" || { printf 'missing %s in %s\n' "$2" "$1" >&2; failed=1; }; }
lacks() { ! grep -Fiq -- "$2" "$1" || { printf 'legacy %s in %s\n' "$2" "$1" >&2; failed=1; }; }

has "$issue" 'ticket start <KEY>'
has "$issue" 'ticket ready <KEY>'
has "$issue" 'installed skills'
has "$issue" 'proportional'
has "$issue" 'before implementation'
has "$issue" 'confidence: high | medium | low'
has "$issue" 'review: skip | run'
has "$issue" 'durable lesson'
has "$review" 'once by default'
has "$review" 'compounds expensively'
has "$review" 'pass 2'
for claim in behavior bug performance refactor 'build or config' 'static property'; do
    has "$verify" "$claim"
done
has "$verify" 'test-first'
lacks "$issue" 'cliban issue tick'
lacks "$issue" '**Red**'
lacks "$issue" '**Green**'
lacks "$issue" 'per-behavior'
lacks "$verify" 'cite'
lacks "$verify" 'citation'
for file in "$workflow" "$milestone" "$setup" "$adapter"; do
    lacks "$file" 'citation'
    lacks "$file" 'citing'
    lacks "$file" 'tdd.md'
done
has "$workflow" 'freeform is valid'
has "$workflow" 'tools, not lifecycle gates'
has "$workflow" 'executable evidence'
has "$workflow" 'once by default'
has "$workflow" 'ticket start KEY'
has "$workflow" 'ticket ready KEY'
has "$setup" 'optionally structured'
has "$adapter" 'optionally structured'

# 75 at CLI-110; +5 for two decided rules — CLI-123's handoff shape and
# CLI-126's ticket-start prime pointer.
lines=$(wc -l <"$issue")
[ "$lines" -le 80 ] || { echo "complete-issue is $lines lines" >&2; failed=1; }

exit "$failed"
