#!/usr/bin/env bash
set -u
DB="$1"; fail=0

ms=$(cliban --db "$DB" milestone ls --project ACME --json | wc -l)
[ "$ms" -eq 1 ] || { echo "FAIL: expected 1 milestone, got $ms (a second one was created instead of filling the existing)"; fail=1; }

n=$(cliban --db "$DB" issue ls --project ACME --milestone "widget caching" --json | wc -l)
[ "$n" -ge 2 ] || { echo "FAIL: expected >=2 tickets on the milestone, got $n"; fail=1; }

orphans=$(cliban --db "$DB" issue ls --project ACME --all --json | jq -r 'select(.milestone == null) | .key' | wc -l)
[ "$orphans" -eq 0 ] || { echo "FAIL: $orphans ticket(s) published outside the milestone"; fail=1; }

# Edges must be real relations, not prose.
edges=$(cliban --db "$DB" issue ls --project ACME --milestone "widget caching" --json \
  | jq -r 'select(.relations != null) | .relations[] | select(.type=="blocks" or .type=="blocked_by") | .type' | wc -l)
[ "$edges" -ge 1 ] || { echo "FAIL: no blocking relations set — edges are relations, never prose"; fail=1; }

for key in $(cliban --db "$DB" issue ls --project ACME --milestone "widget caching" --json | jq -r .key); do
  desc=$(cliban --db "$DB" issue show "$key" --json | jq -r '.description // ""')
  printf '%s' "$desc" | grep -q '^## Spec' \
    || { echo "FAIL: $key has no '## Spec' section"; fail=1; }
  printf '%s' "$desc" | grep -qi '^## Plan' \
    && { echo "FAIL: $key carries a '## Plan' — planning belongs to the executor"; fail=1; }
  printf '%s' "$desc" | grep -qiE '^\*?\*?blocked by:?' \
    && { echo "FAIL: $key states blocking as prose instead of a relation"; fail=1; }
done

exit $fail
