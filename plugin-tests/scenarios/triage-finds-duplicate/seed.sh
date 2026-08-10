#!/usr/bin/env bash
set -eu
DB="$1"
cliban --db "$DB" project add ACME "Acme" --description "test fixture project"
cliban --db "$DB" issue add "Login returns 500 when password field is empty" \
  --project ACME --label bug --priority high --description-file - <<'EOF'
## Spec

**Symptom:** POST /login with an empty password returns HTTP 500 instead of 400.

**Reproduction:**
    curl -X POST localhost:8080/login -d 'user=alice&password='

**Expected:** 400 with a validation message.
**Actual:** 500, stack trace in the log.
EOF
