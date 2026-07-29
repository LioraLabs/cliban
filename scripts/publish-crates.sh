#!/bin/sh
# Publish the workspace to crates.io, bottom-up.
#
# Order matters: a crate cannot be published until everything it depends on is
# already on the registry. `cargo publish` blocks until each one appears in the
# index, so the next step resolves.
#
# Already-published versions are skipped rather than failed, so a partial run
# (network drop halfway) is safe to re-run.
set -eu

ORDER="cliban-core cliban-tui cliban-tenancy cliban cliban-server"

fail() { printf 'publish: %s\n' "$*" >&2; exit 1; }

version=$(cargo metadata --format-version 1 --no-deps |
	python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"]=="cliban"))')

command -v cargo >/dev/null || fail "cargo not on PATH"

# Fail early and legibly rather than after the first upload attempt.
if ! curl -fsS -o /dev/null -H "Authorization: $(python3 -c '
import os, tomllib
p = os.path.expanduser("~/.cargo/credentials.toml")
try:
    print(tomllib.load(open(p, "rb"))["registry"]["token"])
except Exception:
    print("")
')" https://crates.io/api/v1/me 2>/dev/null; then
	fail "crates.io rejected the stored token.
Mint one at https://crates.io/settings/tokens (scopes: publish-new, publish-update)
then run: cargo login"
fi

printf 'publishing cliban %s to crates.io\n\n' "$version"

for crate in $ORDER; do
	if curl -fsS -o /dev/null "https://crates.io/api/v1/crates/$crate/$version" 2>/dev/null; then
		printf '  =  %s %s already published\n' "$crate" "$version"
		continue
	fi
	printf '  →  %s %s\n' "$crate" "$version"
	cargo publish -p "$crate"
done

printf '\nall five crates are on crates.io. `cargo install cliban` and\n'
printf '`cargo binstall cliban` now work.\n'
