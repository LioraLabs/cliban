#!/usr/bin/env bash
# review outcomes survive failed direct delivery.
# the assembled milestone gets its own durable acceptance gate.
set -uo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
review=$ROOT/plugin-flow/skills/complete-issue/references/review.md
issue=$ROOT/plugin-flow/skills/complete-issue/SKILL.md
milestone=$ROOT/plugin-flow/skills/complete-milestone/SKILL.md
workflow=$ROOT/plugin-flow/skills/cliban-workflow/SKILL.md
failed=0

has() { grep -Fiq -- "$2" "$1" || { printf 'missing %s in %s\n' "$2" "$1" >&2; failed=1; }; }
lacks() { ! grep -Fiq -- "$2" "$1" || { printf 'unexpected %s in %s\n' "$2" "$1" >&2; failed=1; }; }

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

has "$issue" 'confidence: high | medium | low'
has "$issue" 'review: skip | run'
has "$issue" 'one-line evidence'
has "$issue" 'no numeric score'
has "$issue" 'Standalone'
has "$issue" 'without an orchestrator waiver'
has "$milestone" 'final decision at every confidence level'
has "$milestone" 'review waived by orchestrator: <reason>'
has "$milestone" 'Either side may request review'
has "$milestone" 'fresh assembled milestone review remains mandatory'
has "$review" 'once by default'
lacks "$review" 'optional.'
lacks "$review" 'marker has exactly one gate'
has "$review" 'checkpoint-free plan gets one cumulative review'
has "$workflow" 'once by default'
lacks "$workflow" 'plan with no markers has one gate at the end'

exit "$failed"
