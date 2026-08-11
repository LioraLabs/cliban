#!/usr/bin/env bash
# CLI-77 — review outcomes survive failed direct delivery.
# CLI-92 — the assembled milestone gets its own durable acceptance gate.
set -uo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
review=$ROOT/plugin-flow/skills/complete-issue/references/review.md
milestone=$ROOT/plugin-flow/skills/complete-milestone/SKILL.md
failed=0

has() { grep -Fiq -- "$2" "$1" || { printf 'missing %s in %s\n' "$2" "$1" >&2; failed=1; }; }

has "$review" 'agentId'
has "$review" 'agent type is never an address'
has "$review" 'verdict and findings summary'
has "$review" 'before sending the full review'
has "$review" 'Full review text stays off the board'
has "$milestone" 'Stranded reviews are expected'
has "$milestone" 'relay'
# shellcheck disable=SC2016
has "$milestone" 'milestone-relevant `## Spec` amendments'
has "$milestone" 'fresh-context reviewer'
has "$milestone" 'assembled milestone branch'
has "$milestone" 'cliban milestone log'
has "$milestone" 'Do not offer finalize until the reviewer passes'

exit "$failed"
