#!/usr/bin/env bash
set -u
DB="$1"; fail=0

key=$(cliban --db "$DB" issue ls --project ACME --all --json | jq -r '.key' | head -1)
[ -n "$key" ] || { echo "FAIL: no issue on the board"; exit 1; }

desc=$(cliban --db "$DB" issue show "$key" --json | jq -r '.description // ""')

# The plan landed...
printf '%s' "$desc" | grep -q '^## Plan' \
  || { echo "FAIL: no '## Plan' section — the first plan write needs --create-section"; fail=1; }
printf '%s' "$desc" | grep -qE '^### Task 1:' \
  || { echo "FAIL: plan has no '### Task 1:' heading, so tick cannot address it"; fail=1; }
printf '%s' "$desc" | grep -qE '^- \[ \]' \
  || { echo "FAIL: plan has no column-zero checkbox steps"; fail=1; }

# ...without destroying the spec it was written against (trap 1).
printf '%s' "$desc" | grep -q '^## Spec' \
  || { echo "FAIL: '## Spec' was destroyed — a whole-description write instead of --section plan"; fail=1; }
for phrase in 'Retry-After' 'per-IP, not global' 'fixed window, not sliding'; do
  printf '%s' "$desc" | grep -qF "$phrase" \
    || { echo "FAIL: spec content lost: '$phrase'"; fail=1; }
done

# lint must agree the structure is tickable.
cliban --db "$DB" issue lint "$key" >/dev/null 2>&1 \
  || { echo "FAIL: issue lint rejects the plan structure"; fail=1; }

exit $fail
