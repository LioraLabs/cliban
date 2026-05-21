#!/usr/bin/env bash
set -euo pipefail
DB=$(mktemp -t cliban-XXXXXX).db
trap 'rm -f "$DB" "$DB"-wal "$DB"-shm' EXIT
export CLIBAN_DB="$DB"

go build -o cliban ./cmd/cliban
./cliban init

./cliban project add CLI --name "Cliban" --description "self-host"
./cliban milestone add --project CLI --name v0.1 --target 2026-06-01

out=$(./cliban issue add --project CLI --title "First" --priority high --json)
echo "$out" | grep -q '"key": "CLI-1"' || { echo "missing key in: $out"; exit 1; }

./cliban issue add --project CLI --title "Sub" --parent CLI-1

set +e
./cliban issue add --project CLI --title "Grand" --parent CLI-2 2>/dev/null
code=$?
set -e
[ "$code" -eq 2 ] || { echo "expected exit 2 on depth-3, got $code"; exit 1; }

./cliban issue mv CLI-1 in-progress
./cliban issue mv CLI-1 done
./cliban issue show CLI-1 --json | grep -q '"status": "done"'

./cliban issue rm CLI-1
remaining=$(./cliban issue ls --project CLI --json | wc -l)
[ "$remaining" -eq 0 ] || { echo "cascading delete failed: $remaining lines"; exit 1; }

# archive
./cliban project add ARC --name "Archive demo"
./cliban issue add --project ARC --title "done1"
./cliban issue add --project ARC --title "done2"
./cliban issue add --project ARC --title "open"
./cliban issue mv ARC-1 done
./cliban issue mv ARC-2 done
n=$(./cliban issue archive-done --project ARC --json | grep -o '"archived": [0-9]*' | grep -o '[0-9]*$')
[ "$n" = "2" ] || { echo "expected archive-done=2, got $n"; exit 1; }
visible=$(./cliban issue ls --project ARC --json | grep -c '"key":')
[ "$visible" -eq 1 ] || { echo "expected 1 visible after archive-done, got $visible"; exit 1; }
all=$(./cliban issue ls --project ARC --archived --json | grep -c '"key":')
[ "$all" -eq 3 ] || { echo "expected 3 with --archived, got $all"; exit 1; }

# fuzzy search
./cliban project add FFF --name "Fuzzy demo"
./cliban issue add --project FFF --title "fuzzy ticket finder"
./cliban issue add --project FFF --title "decoy ticket"

# issue ls --search returns a score field in NDJSON
score_line=$(./cliban issue ls --project FFF --search fuzzy --json | grep '"key":"FFF-1"' || true)
[ -n "$score_line" ] || { echo "ls --search missing FFF-1"; exit 1; }
echo "$score_line" | grep -q '"score":' || { echo "ls --search missing score field"; exit 1; }

# empty --search errors with non-zero exit
set +e
./cliban issue ls --search "  " >/dev/null 2>&1
code=$?
set -e
[ "$code" -ne 0 ] || { echo "expected non-zero exit for empty --search"; exit 1; }

# fff batch mode (stdin not a TTY because we're in a shell pipeline)
fff_out=$(./cliban fff fuzzy </dev/null)
echo "$fff_out" | grep -q '"key":"FFF-1"' || { echo "fff batch missing FFF-1"; exit 1; }
echo "$fff_out" | grep -q '"score":' || { echo "fff batch missing score"; exit 1; }

# fff with no QUERY in non-TTY mode errors
set +e
./cliban fff </dev/null >/dev/null 2>&1
code=$?
set -e
[ "$code" -ne 0 ] || { echo "expected non-zero exit for fff with no QUERY (non-TTY)"; exit 1; }

# --show/--edit/--json mutex
set +e
./cliban fff --show --edit foo >/dev/null 2>&1
code=$?
set -e
[ "$code" -ne 0 ] || { echo "expected non-zero exit for fff with mutex flags"; exit 1; }

echo "smoke ok"
