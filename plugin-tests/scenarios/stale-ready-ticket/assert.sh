#!/usr/bin/env bash
# CLI-85
set -u
DB="$1"; fail=0
KEY=ACME-1

status=$(cliban --db "$DB" issue show "$KEY" --json | jq -r .status)
[ "$status" != "done" ] \
  || { echo "FAIL: stale ticket reached done"; fail=1; }

activity=$(cliban --db "$DB" issue cat "$KEY" --section activity 2>/dev/null)
printf '%s' "$activity" | grep -qF "[cliban-flow] ticket integrate $KEY: refused" \
  || { echo "FAIL: no cliban-flow integrate refusal was recorded"; fail=1; }
printf '%s' "$activity" | grep -q 'milestone/release-train@.* is not an ancestor of' \
  || { echo "FAIL: refusal does not record the failed strict-ancestry relation"; fail=1; }

exit $fail
