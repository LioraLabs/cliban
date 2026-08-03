#!/usr/bin/env bash
set -u
DB="$1"; fail=0

n=$(cliban --db "$DB" issue ls --project ACME --json 2>/dev/null | wc -l)
if [ "$n" -ne 1 ]; then
  echo "FAIL: expected exactly 1 issue on the board, got $n"
  exit 1
fi

key=$(cliban --db "$DB" issue ls --project ACME --json | jq -r '.key')
show=$(cliban --db "$DB" issue show "$key" --json)

printf '%s' "$show" | jq -e '.labels | index("bug")' >/dev/null \
  || { echo "FAIL: issue has no 'bug' label (labels: $(printf '%s' "$show" | jq -c .labels))"; fail=1; }

printf '%s' "$show" | jq -e '.priority == "high" or .priority == "urgent"' >/dev/null \
  || { echo "FAIL: priority is $(printf '%s' "$show" | jq -r .priority), expected high/urgent"; fail=1; }

desc=$(printf '%s' "$show" | jq -r '.description // ""')
printf '%s' "$desc" | grep -qi 'test_ordering' \
  || { echo "FAIL: description does not capture the failing test name"; fail=1; }

exit $fail
