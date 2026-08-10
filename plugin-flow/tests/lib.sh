#!/usr/bin/env bash
# Hermetic fixtures and assertions for the cliban-flow shell tests.
#
# The script under test manipulates git branches, worktrees and the cliban
# board, so a test that escapes its fixture destroys real work. Two rules make
# that impossible rather than unlikely:
#
#   * every fixture repo is created fresh under `mktemp -d` and removed after
#   * CLIBAN_DB is pointed at a file inside that directory before any cliban
#     call, and `fixture_new` aborts the whole run if it is not
#
# Layout: each `*.test.sh` beside this file sources it, calls `fixture_new` per
# case, and ends with `finish`. Adding a subcommand means adding a file, never
# editing one.

set -uo pipefail

TESTS_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
FLOW_BIN="$TESTS_DIR/../scripts/cliban-flow"

FAILURES=0
CHECKS=0
FIXTURE_ROOT=""
FIXTURE_REPO=""
FLOW_OUT=""
FLOW_STATUS=0

# ---------------------------------------------------------------- fixtures

# fixture_new — a throwaway git repo plus a throwaway board, wired together.
#
# The repo gets `main` with one commit, a `milestone/test-milestone` branch off
# it, and nothing else; each case builds the history it needs on top. The board
# gets project FLOW and milestone "Test milestone", whose slug is the milestone
# branch name the script under test is expected to derive.
fixture_new() {
    fixture_cleanup
    FIXTURE_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/cliban-flow-test.XXXXXX") || exit 1
    FIXTURE_REPO="$FIXTURE_ROOT/repo"

    # The one line standing between this suite and the user's real board.
    export CLIBAN_DB="$FIXTURE_ROOT/board.db"
    case "$CLIBAN_DB" in
        "$FIXTURE_ROOT"/*) : ;;
        *) echo "ABORT: CLIBAN_DB escaped the fixture: $CLIBAN_DB" >&2; exit 1 ;;
    esac
    unset CLIBAN_PROJECT
    export CLIBAN_ACTOR="test:cliban-flow"

    mkdir -p "$FIXTURE_REPO"
    git -C "$FIXTURE_REPO" init -q -b main
    git -C "$FIXTURE_REPO" config user.email test@example.invalid
    git -C "$FIXTURE_REPO" config user.name "cliban-flow tests"
    commit_file base.txt "base"
    git -C "$FIXTURE_REPO" branch milestone/test-milestone main

    cb project add FLOW "Flow" >/dev/null
    cb milestone add "Test milestone" --project FLOW >/dev/null
}

fixture_cleanup() {
    [ -n "$FIXTURE_ROOT" ] || return 0
    case "$FIXTURE_ROOT" in
        /tmp/cliban-flow-test.*|"${TMPDIR%/}"/cliban-flow-test.*)
            rm -rf "$FIXTURE_ROOT" ;;
        *) echo "ABORT: refusing to remove unexpected fixture path: $FIXTURE_ROOT" >&2 ;;
    esac
    FIXTURE_ROOT=""
}
trap fixture_cleanup EXIT

# cb <args> — cliban against the fixture board, never the real one.
cb() { cliban --db "$CLIBAN_DB" "$@"; }

# gitf <args> — git inside the fixture repo.
gitf() { git -C "$FIXTURE_REPO" "$@"; }

commit_file() {
    printf '%s\n' "$2" >>"$FIXTURE_REPO/$1"
    gitf add "$1"
    gitf commit -qm "$1: $2"
}

# new_issue <title> — an issue on the fixture milestone; echoes its key.
new_issue() {
    cb issue add "$1" --project FLOW --milestone "Test milestone" --json \
        | json_get key
}

# new_issue_no_milestone <title> — echoes its key.
new_issue_no_milestone() {
    cb issue add "$1" --project FLOW --json | json_get key
}

branch_of() { cb issue show "$1" --json | json_get git_branch_name; }

json_get() {
    python3 -c 'import json,sys
d = json.load(sys.stdin)
v = d.get(sys.argv[1])
sys.stdout.write("" if v is None else str(v))' "$1"
}

# ---------------------------------------------------------------- invocation

# run_flow <args> — run the dispatcher from inside the fixture repo, capturing
# stdout and stderr together into FLOW_OUT and the status into FLOW_STATUS.
# A caller that needs a different cwd sets FLOW_CWD first.
run_flow() {
    local cwd=${FLOW_CWD:-$FIXTURE_REPO}
    FLOW_OUT=$(cd "$cwd" && "$FLOW_BIN" "$@" 2>&1)
    FLOW_STATUS=$?
}

# ---------------------------------------------------------------- assertions

pass() { CHECKS=$((CHECKS + 1)); printf 'ok - %s\n' "$1"; }

fail() {
    CHECKS=$((CHECKS + 1))
    FAILURES=$((FAILURES + 1))
    printf 'not ok - %s\n' "$1"
    printf '%s\n' "$2" | sed 's/^/    /'
}

assert_status() {
    if [ "$FLOW_STATUS" -eq "$1" ]; then
        pass "$2"
    else
        fail "$2" "expected exit $1, got $FLOW_STATUS. Output:
$FLOW_OUT"
    fi
}

assert_eq() {
    if [ "$1" = "$2" ]; then
        pass "$3"
    else
        fail "$3" "expected: $2
     got: $1"
    fi
}

assert_out_has() {
    if printf '%s' "$FLOW_OUT" | grep -qF -- "$1"; then
        pass "$2"
    else
        fail "$2" "expected output to contain: $1
Output:
$FLOW_OUT"
    fi
}

assert_out_lacks() {
    if printf '%s' "$FLOW_OUT" | grep -qF -- "$1"; then
        fail "$2" "expected output NOT to contain: $1
Output:
$FLOW_OUT"
    else
        pass "$2"
    fi
}

# assert_board_has <KEY> <substring> <desc> — the activity log is a shipped
# interface, not a side effect: recovery reads it and a behavioural scenario
# greps it, so the tests assert on it.
assert_board_has() {
    local log
    log=$(cb issue cat "$1" --section activity 2>&1)
    if printf '%s' "$log" | grep -qF -- "$2"; then
        pass "$3"
    else
        fail "$3" "expected $1's activity log to contain: $2
Activity log:
$log"
    fi
}

# ---------------------------------------------------------------- driver

finish() {
    printf '# %s: %d checks, %d failed\n' "$(basename "$0")" "$CHECKS" "$FAILURES"
    [ "$FAILURES" -eq 0 ]
    exit $?
}
