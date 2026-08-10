#!/usr/bin/env bash
set -eu
DB="$1"
cliban --db "$DB" project add ACME "Acme" --description "test fixture project"
cliban --db "$DB" issue add "Rate-limit the login endpoint" --project ACME \
  --label feature --priority medium --description-file - <<'EOF'
## Spec

**What it delivers:** the login endpoint rejects a 6th attempt within 60s from
one IP with HTTP 429 and a Retry-After header.

**Acceptance criteria:**
- 5 attempts in 60s succeed normally
- the 6th returns 429 with Retry-After
- the window is per-IP, not global

**Decisions:** fixed window, not sliding — agreed during scoping.
EOF
