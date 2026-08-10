#!/usr/bin/env bash
# CLI-77 — review outcomes survive failed direct delivery.
set -uo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
issue=$ROOT/plugin-flow/skills/complete-issue/SKILL.md
review=$ROOT/plugin-flow/skills/complete-issue/references/review.md
milestone=$ROOT/plugin-flow/skills/complete-milestone/SKILL.md
failed=0

has() { grep -Fiq -- "$2" "$1" || { printf 'missing %s in %s\n' "$2" "$1" >&2; failed=1; }; }

has "$review" 'agentId'
has "$review" 'agent type is never an address'
has "$review" 'verdict and findings summary'
has "$review" 'before sending the full review'
has "$review" 'Full review text stays off the board'
has "$issue" 'verdict is still in flight'
has "$issue" 'via the orchestrator'
has "$milestone" 'Stranded reviews are expected'
has "$milestone" 'relay'

exit "$failed"
