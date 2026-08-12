#!/usr/bin/env bash
set -u
DB="$1"; fail=0

n=$(cliban --db "$DB" issue ls --project ACME --all --json | wc -l)
if [ "$n" -ne 1 ]; then
  echo "FAIL: expected the existing ticket to be reused, got $n issues (a duplicate was filed)"
  cliban --db "$DB" issue ls --project ACME --all --json | jq -r '"  - " + .key + " " + .title'
  fail=1
fi

key=$(cliban --db "$DB" issue ls --project ACME --all --json | jq -r '.key' | head -1)

# The new evidence has to land somewhere on the existing ticket — activity log
# or an added section — not be silently dropped.
merged=$(cliban --db "$DB" activity --issue "$key" --json | jq -r '.message // ""' | grep -ci 'NullPointerException\|AuthService' || true)
desc=$(cliban --db "$DB" issue show "$key" --json | jq -r '.description // ""')
indesc=$(printf '%s' "$desc" | grep -ci 'NullPointerException\|AuthService' || true)

if [ "$merged" -eq 0 ] && [ "$indesc" -eq 0 ]; then
  echo "FAIL: the new stack-trace evidence was not added to $key (neither activity log nor description)"
  fail=1
fi

exit $fail
