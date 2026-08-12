#!/usr/bin/env bash
set -uo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
issue=$ROOT/plugin-flow/skills/complete-issue/SKILL.md
review=$ROOT/plugin-flow/skills/complete-issue/references/review.md
verify=$ROOT/plugin-flow/skills/complete-issue/references/verification.md
failed=0

has() { grep -Fiq -- "$2" "$1" || { printf 'missing %s in %s\n' "$2" "$1" >&2; failed=1; }; }
lacks() { ! grep -Fiq -- "$2" "$1" || { printf 'legacy %s in %s\n' "$2" "$1" >&2; failed=1; }; }

has "$issue" 'ticket start <KEY>'
has "$issue" 'ticket ready <KEY>'
has "$issue" 'installed skills'
has "$issue" 'proportional'
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

lines=$(wc -l <"$issue")
[ "$lines" -le 75 ] || { echo "complete-issue is $lines lines" >&2; failed=1; }

exit "$failed"
