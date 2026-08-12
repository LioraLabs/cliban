#!/usr/bin/env bash
set -u
DB="$1"; fail=0
KEY="ACME-1"

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
