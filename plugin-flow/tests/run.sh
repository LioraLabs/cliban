#!/usr/bin/env bash
# Run every cliban-flow shell test. Each `*.test.sh` beside this file is a
# separate process, so one suite's fixture cannot leak into the next and one
# crash cannot take the run with it.
#
#   plugin-flow/tests/run.sh              everything
#   plugin-flow/tests/run.sh ticket-status  one suite, by name or path
set -uo pipefail

TESTS_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)

command -v cliban >/dev/null || { echo "cliban is not on PATH" >&2; exit 1; }
command -v git >/dev/null || { echo "git is not on PATH" >&2; exit 1; }

suites=()
if [ $# -gt 0 ]; then
    for arg in "$@"; do
        if [ -f "$arg" ]; then suites+=("$arg")
        elif [ -f "$TESTS_DIR/$arg.test.sh" ]; then suites+=("$TESTS_DIR/$arg.test.sh")
        else echo "no such suite: $arg" >&2; exit 1
        fi
    done
else
    for f in "$TESTS_DIR"/*.test.sh; do [ -f "$f" ] && suites+=("$f"); done
fi

[ ${#suites[@]} -gt 0 ] || { echo "no test suites found in $TESTS_DIR" >&2; exit 1; }

failed=0
for suite in "${suites[@]}"; do
    printf '# %s\n' "$(basename "$suite")"
    bash "$suite" || failed=$((failed + 1))
done

if [ "$failed" -ne 0 ]; then
    printf '%d of %d suite(s) failed\n' "$failed" "${#suites[@]}" >&2
    exit 1
fi
printf 'all %d suite(s) passed\n' "${#suites[@]}"
