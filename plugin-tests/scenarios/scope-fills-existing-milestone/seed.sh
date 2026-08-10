#!/usr/bin/env bash
set -eu
DB="$1"
cliban --db "$DB" project add ACME "Acme" --description "test fixture project"
cliban --db "$DB" milestone add "widget caching" --project ACME --description-file - <<'EOF'
## Spec

**Problem:** widget lookups hit the database on every request.

**Approach:** add a read-through cache in front of the widget repository.

**In scope:** the cache layer, its invalidation on write, and metrics.
**Out of scope:** caching anything other than widgets.

**Open decisions:** eviction policy; whether metrics ship in the same slice.
EOF
