#!/bin/sh
# Tag a release and push it. The tag push is what triggers .github/workflows/release.yml,
# which re-checks the version, runs the suite, and publishes the tarballs + SHA256SUMS.
#
#     scripts/release.sh 0.3.0
#
# Run by `cook release`, which gates it on `cook check`. Everything asserted here
# is asserted again in CI — this copy exists so you find out before the tag is
# public rather than after.
set -eu

REMOTE="${CLIBAN_REMOTE:-github}"
BRANCH="${CLIBAN_BRANCH:-main}"

fail() { printf 'release: %s\n' "$*" >&2; exit 1; }

[ $# -eq 1 ] || fail "usage: release.sh <version>   e.g. 0.3.0"
version="${1#v}"
tag="v$version"

manifest=$(cargo metadata --format-version 1 --no-deps |
	python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"]=="cliban"))')

[ "$manifest" = "$version" ] ||
	fail "asked for $tag but Cargo.toml says $manifest — run: cook bump $version"

git diff-index --quiet HEAD -- ||
	fail "working tree is dirty; commit the bump first"

[ -z "$(git status --porcelain --untracked-files=normal)" ] ||
	fail "untracked files present; commit or ignore them first"

current=$(git rev-parse --abbrev-ref HEAD)
[ "$current" = "$BRANCH" ] ||
	fail "on branch '$current', expected '$BRANCH' (override: CLIBAN_BRANCH)"

! git rev-parse -q --verify "refs/tags/$tag" >/dev/null ||
	fail "tag $tag already exists locally"

! git ls-remote --exit-code --tags "$REMOTE" "$tag" >/dev/null 2>&1 ||
	fail "tag $tag already exists on $REMOTE — releases are immutable, bump instead"

printf 'releasing cliban %s to %s\n' "$tag" "$REMOTE"

git push "$REMOTE" "$BRANCH"
git tag -a "$tag" -m "cliban $tag"
git push "$REMOTE" "$tag"

printf '\ntag pushed. The Release workflow is building:\n'
printf '  gh run watch $(gh run list --workflow Release --limit 1 --json databaseId --jq ".[0].databaseId")\n'
printf '\nOnce it is green:\n'
printf '  cook tap %s      # refresh the Homebrew formula\n' "$version"
printf '  cook aur %s      # refresh the AUR checksums\n' "$version"
printf '  cook publish        # push the crates to crates.io\n'
