#!/usr/bin/env bash
set -eu
DB="$1"
cliban --db "$DB" project add ACME "Acme" --description-file - <<'EOF'
Test fixture project.

## Notes

### SQLite schema diffs need canonical form
Always compare canonicalized SQL (whitespace + quoting normalized) before
diffing schemas; raw .schema output produces false positives.
EOF
