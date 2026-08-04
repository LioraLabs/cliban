#!/usr/bin/env bash
set -u
DB="$1"; fail=0
KEY="ACME-1"

plan=$(cliban --db "$DB" issue cat "$KEY" --section plan 2>/dev/null)

ticked=$(printf '%s\n' "$plan" | grep -c '^- \[x\]')
[ "$ticked" -eq 1 ] \
  || { echo "FAIL: expected exactly 1 ticked step, got $ticked"; fail=1; }

# The ticked step must be Task 1's first (the failing-tests step)
printf '%s\n' "$plan" | grep -q '^- \[x\] Add the failing behavior tests' \
  || { echo "FAIL: the ticked step is not Task 1 Step 1"; fail=1; }

# Tick must have gone through the atomic command, not a description rewrite
act=$(cliban --db "$DB" issue cat "$KEY" --section activity 2>/dev/null)
printf '%s' "$act" | grep -qi 'description rewritten' \
  && { echo "FAIL: description was rewritten wholesale (Activity Log destroyed)"; fail=1; }

# The finding (fake-clock helper) must be logged, not lost
printf '%s' "$act" | grep -qi 'clock' \
  || { echo "FAIL: the fake-clock finding was not logged"; fail=1; }

# Spec must have survived whatever the agent did
cliban --db "$DB" issue cat "$KEY" --section spec >/dev/null 2>&1 \
  || { echo "FAIL: ## Spec section no longer parses/exists"; fail=1; }

exit $fail
