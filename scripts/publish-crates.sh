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

# crates.io 403s any request without a real User-Agent, including anonymous
# reads. curl's default counts as unreal, so every probe below must set one.
UA="cliban-release/$version"

# Only assert a token EXISTS. Do not try to validate it: crates.io tokens are
# endpoint-scoped, and one scoped to publish-new/publish-update is legitimately
# 403'd by /api/v1/me — which made an earlier version of this check call a
# perfectly good token dead. cargo reports real auth failures well enough.
[ -n "${CARGO_REGISTRY_TOKEN:-}" ] ||
	[ -f "$HOME/.cargo/credentials.toml" ] ||
	[ -f "$HOME/.cargo/credentials" ] ||
	fail "no crates.io token found.
Mint one at https://crates.io/settings/tokens (scopes: publish-new, publish-update)
then run: cargo login"

printf 'publishing cliban %s to crates.io\n\n' "$version"

for crate in $ORDER; do
	# 200 = this exact version is up, 404 = it is not. Anything else is a
	# question we cannot answer, and guessing "not published" would mean an
	# upload attempt that fails halfway through the chain.
	code=$(curl -s -o /dev/null -w '%{http_code}' -A "$UA" \
		"https://crates.io/api/v1/crates/$crate/$version")
	case "$code" in
		200)
			printf '  =  %s %s already published\n' "$crate" "$version"
			continue
			;;
		404) ;;
		*) fail "crates.io returned $code for $crate/$version; refusing to guess" ;;
	esac
	printf '  →  %s %s\n' "$crate" "$version"
	cargo publish -p "$crate"
done

printf '\nall five crates are on crates.io.\n'
printf 'cargo install cliban and cargo binstall cliban now work.\n'
