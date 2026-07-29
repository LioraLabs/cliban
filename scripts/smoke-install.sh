#!/bin/sh
# Run install.sh end-to-end against the live release, into a sandbox prefix.
#
# Piped through `sh` rather than executed directly, because that is what the
# README tells people to run and it is the path that has to keep working.
# Touches nothing outside its temp prefix.
set -eu

RAW="${CLIBAN_INSTALL_URL:-https://raw.githubusercontent.com/LioraLabs/cliban/main/install.sh}"

fail() { printf 'smoke: %s\n' "$*" >&2; exit 1; }

prefix=$(mktemp -d)
trap 'rm -rf "$prefix"' EXIT INT TERM

printf -- '--- installing from %s ---\n' "$RAW"
curl -fsSL "$RAW" | CLIBAN_BIN_DIR="$prefix" sh || fail "installer exited non-zero"

[ -x "$prefix/cliban" ] || fail "cliban was not installed"
[ -x "$prefix/cliband" ] || fail "cliband was not installed"

printf -- '\n--- versions ---\n'
"$prefix/cliban" --version || fail "cliban --version failed"
"$prefix/cliband" --version || fail "cliband --version failed"

# The installed binary should match the newest published release, which is what
# a first-time user gets.
latest=$(curl -fsSLI -o /dev/null -w '%{url_effective}' \
	https://github.com/LioraLabs/cliban/releases/latest)
latest=${latest##*/}
got=$("$prefix/cliban" --version | awk '{print $2}')

[ "v$got" = "$latest" ] ||
	fail "installed cliban $got but the latest release is $latest"

printf '\nok: install.sh delivers %s\n' "$latest"
