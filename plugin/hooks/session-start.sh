#!/usr/bin/env bash
# SessionStart: inject live board state when this repo is bound to cliban.
#
# Philosophy: the failure mode this hook exists to prevent is an agent that
# never consults the board, because nothing in "fix the flaky test" triggers a
# skill. So it injects STATE (a few lines), not doctrine — the skills carry the
# doctrine. Exits 0 silently everywhere that isn't a cliban-bound repo.
set -u

command -v cliban >/dev/null 2>&1 || exit 0
command -v jq     >/dev/null 2>&1 || exit 0
ROOT=$(git rev-parse --show-toplevel 2>/dev/null) || exit 0
ADAPTER="$ROOT/docs/agents/issue-tracker.md"
[ -f "$ADAPTER" ] || exit 0
grep -qi '^# Issue tracker: cliban' "$ADAPTER" || exit 0

KEY=$(grep -oE '\*\*Project key:\*\* *[A-Z][A-Z0-9]{1,9}' "$ADAPTER" | grep -oE '[A-Z][A-Z0-9]{1,9}$' | head -1)
[ -n "$KEY" ] || exit 0

echo "This repo is bound to cliban project ${KEY} (binding: docs/agents/issue-tracker.md)."

CUR=$(cliban issue current --json 2>/dev/null) \
  && echo "Current branch issue: $(printf '%s' "$CUR" | jq -r '"\(.key) [\(.status)] \(.title)"')"

IP=$(cliban issue ls --project "$KEY" --status in-progress --json 2>/dev/null \
  | jq -r '"  \(.key) \(.title)"' | head -5)
[ -n "$IP" ] && printf 'In progress:\n%s\n' "$IP"

BL=$(cliban issue ls --blocked --project "$KEY" --json 2>/dev/null \
  | jq -r '"  \(.key) \(.title)"' | head -3)
[ -n "$BL" ] && printf 'Blocked:\n%s\n' "$BL"

LAST=$(cliban activity --project "$KEY" --since 7d --limit 1 --json 2>/dev/null \
  | jq -r '"\(.ts) \(.kind) \(.key)"' | head -1)
[ -n "$LAST" ] && echo "Last board activity: ${LAST}"

echo "Track work on the board as you go (cliban skill: mechanics; cliban-workflow skill: the contract)."
exit 0
