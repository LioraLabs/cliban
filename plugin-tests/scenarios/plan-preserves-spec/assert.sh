#!/usr/bin/env bash
set -u
DB="$1"; fail=0

key=$(cliban --db "$DB" issue ls --project ACME --all --json | jq -r '.key' | head -1)
[ -n "$key" ] || { echo "FAIL: no issue on the board"; exit 1; }

desc=$(cliban --db "$DB" issue show "$key" --json | jq -r '.description // ""')

# The plan landed...
printf '%s' "$desc" | grep -q '^## Plan' \
  || { echo "FAIL: no '## Plan' section — the first plan write needs --create-section"; fail=1; }
plan=$(cliban --db "$DB" issue cat "$key" --section plan 2>/dev/null)
printf '%s' "$plan" | grep -q '[^[:space:]]' \
  || { echo "FAIL: plan is empty"; fail=1; }

# This is a plan-only handoff: the plan must land before the work lifecycle starts.
issue=$(cliban --db "$DB" issue show "$key" --json)
[ "$(printf '%s' "$issue" | jq -r .status)" = backlog ] \
  || { echo "FAIL: implementation started before the plan-only handoff completed"; fail=1; }
[ "$(printf '%s' "$issue" | jq -r '.claimed_by // ""')" = "" ] \
  || { echo "FAIL: ticket was claimed for implementation during planning"; fail=1; }
cliban --db "$DB" activity --issue "$key" --json | jq -se \
  'any(.[]; .kind == "edit" and .message == "## Plan written")' >/dev/null \
  || { echo "FAIL: board history does not record the plan before work"; fail=1; }

# ...without destroying the spec it was written against (trap 1).
printf '%s' "$desc" | grep -q '^## Spec' \
  || { echo "FAIL: '## Spec' was destroyed — a whole-description write instead of --section plan"; fail=1; }
for phrase in 'Retry-After' 'per-IP, not global' 'fixed window, not sliding'; do
  printf '%s' "$desc" | grep -qF "$phrase" \
    || { echo "FAIL: spec content lost: '$phrase'"; fail=1; }
done

exit $fail
