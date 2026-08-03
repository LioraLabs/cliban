#!/usr/bin/env bash
set -u
DB="$1"; fail=0

notes=$(cliban --db "$DB" project show ACME --section notes 2>/dev/null)

# The lesson landed as a subsection in project Notes
printf '%s' "$notes" | grep -qi 'test-threads' \
  || { echo "FAIL: lesson not found in project ## Notes"; fail=1; }

# It went in as a new ### subsection, not appended to an existing one
n=$(printf '%s\n' "$notes" | grep -c '^### ')
[ "$n" -ge 2 ] \
  || { echo "FAIL: expected >=2 ### subsections after recording, got $n"; fail=1; }

# The pre-existing lesson survived the round-trip
printf '%s' "$notes" | grep -q 'canonical form' \
  || { echo "FAIL: pre-existing Notes subsection was destroyed"; fail=1; }

# The lesson belongs in project memory, not parked as a fake issue
nissues=$(cliban --db "$DB" issue ls --project ACME --json 2>/dev/null | wc -l)
[ "$nissues" -eq 0 ] \
  || { echo "FAIL: agent created $nissues issue(s) as memory instead of using Notes"; fail=1; }

exit $fail
