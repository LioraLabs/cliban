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
# Layout: each `*.test.sh` beside this file sources it, calls `fixture_new`
# before any case that needs its own history, and ends with `finish`. Adding a
# subcommand means adding a file, never editing one.

set -uo pipefail

TESTS_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
FLOW_BIN="$TESTS_DIR/../scripts/cliban-flow"

FAILURES=0
CHECKS=0
FIXTURE_ROOT=""
FIXTURE_REPO=""
FLOW_OUT=""
FLOW_STDOUT=""
FLOW_STDERR=""
FLOW_STATUS=0

abort() { printf 'ABORT: %s\n' "$1" >&2; exit 1; }

# ---------------------------------------------------------------- fixtures

FIXTURE_PARENT=${TMPDIR:-/tmp}
FIXTURE_PARENT=${FIXTURE_PARENT%/}

# fixture_new — a throwaway git repo plus a throwaway board, wired together.
#
# The repo gets `main` with one commit, a `milestone/test-milestone` branch off
# it, and nothing else; each case builds the history it needs on top. The board
# gets project FLOW and milestone "Test milestone", whose slug is the milestone
# branch name the script under test is expected to derive.
fixture_new() {
    fixture_cleanup
    FIXTURE_ROOT=$(mktemp -d "$FIXTURE_PARENT/cliban-flow-test.XXXXXX") ||
        abort "could not create a fixture directory under $FIXTURE_PARENT"
    FIXTURE_REPO="$FIXTURE_ROOT/repo"

    # The one line standing between this suite and the user's real board.
    export CLIBAN_DB="$FIXTURE_ROOT/board.db"
    case "$CLIBAN_DB" in
        "$FIXTURE_ROOT"/*) : ;;
        *) abort "CLIBAN_DB escaped the fixture: $CLIBAN_DB" ;;
    esac
    unset CLIBAN_PROJECT
    export CLIBAN_ACTOR="test:cliban-flow"

    # The user's git config must not reach the fixture: a global commit.gpgsign
    # or core.hooksPath would break or hijack every commit made below, and an
    # inherited GIT_DIR would point the whole suite at some other repository.
    unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE
    mkdir -p "$FIXTURE_REPO"
    git -C "$FIXTURE_REPO" init -q -b main
    git -C "$FIXTURE_REPO" config user.email test@example.invalid
    git -C "$FIXTURE_REPO" config user.name "cliban-flow tests"
    git -C "$FIXTURE_REPO" config commit.gpgsign false
    git -C "$FIXTURE_REPO" config core.hooksPath /dev/null
    commit_file base.txt "base"
    git -C "$FIXTURE_REPO" branch milestone/test-milestone main

    cb project add FLOW "Flow" >/dev/null
    cb milestone add "Test milestone" --project FLOW >/dev/null
}

fixture_cleanup() {
    [ -n "$FIXTURE_ROOT" ] || return 0
    case "$FIXTURE_ROOT" in
        "$FIXTURE_PARENT"/cliban-flow-test.*) rm -rf "$FIXTURE_ROOT" ;;
        *) printf 'refusing to remove unexpected fixture path: %s\n' "$FIXTURE_ROOT" >&2 ;;
    esac
    FIXTURE_ROOT=""
}
trap fixture_cleanup EXIT

# cb <args> — cliban against the fixture board, never the real one.
cb() { cliban --db "$CLIBAN_DB" "$@"; }

# gitf <args> — git inside the fixture repo.
gitf() { git -C "$FIXTURE_REPO" "$@"; }

commit_file() { commit_file_at "$FIXTURE_REPO" "$1" "$2"; }

# commit_file_at <worktree> <file> <content> — the same, in a linked worktree.
commit_file_at() {
    printf '%s\n' "$3" >>"$1/$2"
    git -C "$1" add "$2"
    git -C "$1" commit -qm "$2: $3"
}

# fixture_milestone_wt — where the script under test is expected to put the
# fixture milestone's worktree: the primary checkout's own path with the
# milestone slug appended. Spelled out here rather than asked of the script, so
# the assertion is a specification and not an echo.
fixture_milestone_wt() { printf '%s' "$FIXTURE_ROOT/repo-test-milestone"; }

# fixture_milestone_worktree — that worktree, built with plain git rather than
# by running `milestone start`. A ticket-start case must fail because ticket
# start is wrong, never because milestone start is.
fixture_milestone_worktree() {
    gitf worktree add -q "$(fixture_milestone_wt)" milestone/test-milestone
}

# fixture_ticket_wt <branch> — where a ticket's worktree belongs: under the
# milestone worktree, named for the branch. Spelled out here rather than asked
# of the script, so an assertion against it is a specification.
fixture_ticket_wt() { printf '%s/.worktrees/%s' "$(fixture_milestone_wt)" "$1"; }

# fixture_ticket_worktree <branch> — that worktree, branched off the fixture
# milestone's tip and built with plain git rather than by running `ticket
# start`. A sync or ready case must fail because sync or ready is wrong, never
# because start is. An existing branch is adopted rather than recreated, so a
# case can lay down history first and ask for a worktree over it.
fixture_ticket_worktree() {
    mkdir -p "$(fixture_milestone_wt)/.worktrees"
    if gitf rev-parse --verify --quiet "refs/heads/$1" >/dev/null; then
        gitf worktree add -q "$(fixture_ticket_wt "$1")" "$1"
    else
        gitf worktree add -q -b "$1" "$(fixture_ticket_wt "$1")" \
            refs/heads/milestone/test-milestone
    fi
}

# gitt <branch> <args>... — git inside a ticket's worktree.
gitt() {
    local branch=$1
    shift
    git -C "$(fixture_ticket_wt "$branch")" "$@"
}

# new_issue <title> [milestone] — an issue on the fixture milestone unless one
# is named; echoes its key. Aborts rather than echoing nothing, so a broken
# fixture cannot make a guard case pass for the wrong reason.
new_issue() {
    local key
    key=$(cb issue add "$1" --project FLOW --milestone "${2:-Test milestone}" --json |
        json_get key)
    [ -n "$key" ] || abort "fixture could not create an issue titled: $1"
    printf '%s' "$key"
}

# new_issue_no_milestone <title> — echoes its key.
new_issue_no_milestone() {
    local key
    key=$(cb issue add "$1" --project FLOW --json | json_get key)
    [ -n "$key" ] || abort "fixture could not create an issue titled: $1"
    printf '%s' "$key"
}

branch_of() {
    local branch
    branch=$(cb issue show "$1" --json | json_get git_branch_name)
    [ -n "$branch" ] || abort "fixture could not read a branch name for $1"
    printf '%s' "$branch"
}

# cliban_slug_of <text> — the slug cliban itself derives, read back off an
# issue's git_branch_name with the key prefix stripped. Independent of the
# script under test, so comparing against it is a real differential rather than
# two hand-rolled slugifiers agreeing with each other.
cliban_slug_of() {
    local key branch
    key=$(new_issue_no_milestone "$1")
    branch=$(branch_of "$key")
    printf '%s' "${branch#"$(printf '%s' "$key" | tr '[:upper:]' '[:lower:]')-"}"
}

json_get() {
    python3 -c 'import json,sys
d = json.load(sys.stdin)
v = d.get(sys.argv[1])
sys.stdout.write("" if v is None else str(v))' "$1"
}

# ---------------------------------------------------------------- invocation

# run_flow <args> — run the dispatcher from inside the fixture repo. The two
# streams are captured separately, because which stream a line lands on is part
# of the interface: a caller does `verdict=$(cliban-flow ticket status KEY)` and
# gets the verdict alone only while the guidance stays on stderr. FLOW_OUT is
# both, for assertions that do not care.
# A caller that needs a different cwd sets FLOW_CWD first, and a different PATH
# FLOW_PATH — the substitutions below apply to the script under test only, so
# the harness keeps the tools it needs to make its own assertions.
run_flow() {
    local cwd=${FLOW_CWD:-$FIXTURE_REPO} errfile
    errfile="$FIXTURE_ROOT/stderr.$$"
    FLOW_STDOUT=$(cd "$cwd" && PATH="${FLOW_PATH:-$PATH}" "$FLOW_BIN" "$@" 2>"$errfile")
    FLOW_STATUS=$?
    FLOW_STDERR=$(cat "$errfile")
    rm -f "$errfile"
    FLOW_OUT="$FLOW_STDOUT
$FLOW_STDERR"
}

# The helpers below substitute an external boundary — the board, or the JSON
# reader — for the script under test only, via stub_bin. They set FLOW_PATH; a
# case ends with `unset FLOW_PATH`. Anything that talks to a tool this suite does not own
# is a legitimate place to substitute; nothing inside the script is.

stub_bin() {
    mkdir -p "$FIXTURE_ROOT/bin"
    printf '%s\n' "$2" >"$FIXTURE_ROOT/bin/$1"
    chmod +x "$FIXTURE_ROOT/bin/$1"
    FLOW_PATH="$FIXTURE_ROOT/bin:$PATH"
}

# break_board_writes — a cliban that fails every `issue log` and passes
# everything else through.
break_board_writes() {
    local real
    real=$(command -v cliban) || abort "cliban is not on PATH"
    # The single quotes are the point: this is a program for the stub's shell to
    # expand, not for this one.
    # shellcheck disable=SC2016
    stub_bin cliban '#!/usr/bin/env bash
if [ "${1:-}" = issue ] && [ "${2:-}" = log ]; then
    echo "stub: the board is unreachable" >&2
    exit 1
fi
exec '"$real"' "$@"'
}

# break_milestone_board_writes — a cliban that fails every `milestone log` and
# passes everything else through, including the `milestone show` the script
# needs to resolve the milestone in the first place.
break_milestone_board_writes() {
    local real
    real=$(command -v cliban) || abort "cliban is not on PATH"
    # shellcheck disable=SC2016
    stub_bin cliban '#!/usr/bin/env bash
if [ "${1:-}" = milestone ] && [ "${2:-}" = log ]; then
    echo "stub: the board is unreachable" >&2
    exit 1
fi
exec '"$real"' "$@"'
}

# forge_branch_name <name> — a cliban whose `issue show --json` reports <name>
# as the issue's git_branch_name and is otherwise itself. The board is the
# boundary being substituted: cliban's own slugifier cannot produce a name that
# escapes a directory, so the only way to exercise a containment guard is to
# have the board say something cliban would never say.
forge_branch_name() {
    local real
    real=$(command -v cliban) || abort "cliban is not on PATH"
    # shellcheck disable=SC2016
    stub_bin cliban '#!/usr/bin/env bash
if [ "${1:-}" = issue ] && [ "${2:-}" = show ]; then
    '"$real"' "$@" | FORGED='"$1"' python3 -c '"'"'import json, os, sys
d = json.load(sys.stdin)
d["git_branch_name"] = os.environ["FORGED"]
print(json.dumps(d))'"'"'
    exit "${PIPESTATUS[0]}"
fi
exec '"$real"' "$@"'
}

# break_json_reader — a python3 that always fails.
break_json_reader() {
    stub_bin python3 '#!/usr/bin/env bash
echo "stub: no json for you" >&2
exit 1'
}

# hide_json_reader — a PATH carrying only what the script needs to reach its own
# python3 guard, so python3 is genuinely absent rather than merely broken. bash
# and env are here because the shebang resolves through PATH too.
hide_json_reader() {
    local tool
    mkdir -p "$FIXTURE_ROOT/bin"
    for tool in bash env git cliban; do
        ln -sf "$(command -v "$tool")" "$FIXTURE_ROOT/bin/$tool"
    done
    FLOW_PATH="$FIXTURE_ROOT/bin"
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

assert_stdout_is() {
    if [ "$FLOW_STDOUT" = "$1" ]; then
        pass "$2"
    else
        fail "$2" "expected stdout to be exactly: $1
                got: $FLOW_STDOUT"
    fi
}

assert_stderr_has() {
    if printf '%s' "$FLOW_STDERR" | grep -qF -- "$1"; then
        pass "$2"
    else
        fail "$2" "expected stderr to contain: $1
Stderr:
$FLOW_STDERR"
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

assert_board_lacks() {
    local log
    log=$(cb issue cat "$1" --section activity 2>&1)
    if printf '%s' "$log" | grep -qF -- "$2"; then
        fail "$3" "expected $1's activity log NOT to contain: $2
Activity log:
$log"
    else
        pass "$3"
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

# assert_milestone_board_has <NAME> <substring> <desc> — the same assertion one
# level up. A subcommand acting on a milestone rather than a ticket writes its
# line to the milestone's own activity log, so proving it landed means reading a
# different object; everything else about the line is identical, because
# board_line() builds both.
assert_milestone_board_has() {
    local log
    log=$(cb milestone show "$1" -p FLOW --json 2>&1 | json_get description)
    if printf '%s' "$log" | grep -qF -- "$2"; then
        pass "$3"
    else
        fail "$3" "expected milestone \"$1\"'s activity log to contain: $2
Description:
$log"
    fi
}

# ---------------------------------------------------------------- driver

finish() {
    printf '# %s: %d checks, %d failed\n' "$(basename "$0")" "$CHECKS" "$FAILURES"
    [ "$FAILURES" -eq 0 ]
    exit $?
}
