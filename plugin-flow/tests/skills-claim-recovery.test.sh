#!/usr/bin/env bash
# CLI-94 — a dead claimant cannot strand a single ticket.
set -uo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
issue=$ROOT/plugin-flow/skills/complete-issue/SKILL.md
diagnose=$ROOT/plugin-flow/skills/diagnose-issue/SKILL.md
failed=0

has() { grep -Fiq -- "$2" "$1" || { printf 'missing %s in %s\n' "$2" "$1" >&2; failed=1; }; }

for text in 'in-progress' 'claimed' '## Plan' '## Activity Log' 'worktree' 'ask the claimant' 'cannot continue' 'issue release <KEY>' 'claim <KEY> --force'; do
    has "$issue" "$text"
done

# shellcheck disable=SC2016
has "$diagnose" 'continue directly into `complete-issue`'
has "$diagnose" 'same session'
has "$diagnose" 'cliban issue release <KEY>'

exit "$failed"
