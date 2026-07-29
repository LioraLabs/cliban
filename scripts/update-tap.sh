#!/bin/sh
# Regenerate the Homebrew formula for a release and push the tap.
#
#     scripts/update-tap.sh 0.3.0
#
# The tap is a separate repo (LioraLabs/homebrew-tap) holding one generated
# file. gen_formula.py there reads the release's SHA256SUMS, so this never
# hand-copies a checksum.
set -eu

TAP="${CLIBAN_TAP:-LioraLabs/homebrew-tap}"

fail() { printf 'tap: %s\n' "$*" >&2; exit 1; }

[ $# -eq 1 ] || fail "usage: update-tap.sh <version>   e.g. 0.3.0"
version="${1#v}"
tag="v$version"

command -v gh >/dev/null || fail "gh not on PATH"

# The formula points at release assets; if they are not up yet it would pin
# checksums for a half-published release.
gh release view "$tag" --repo LioraLabs/cliban >/dev/null 2>&1 ||
	fail "no $tag release yet — wait for the Release workflow to finish"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT INT TERM

git clone -q "https://github.com/$TAP.git" "$work/tap" || fail "could not clone $TAP"

python3 "$work/tap/gen_formula.py" "$tag" > "$work/cliban.rb" ||
	fail "formula generation failed (is SHA256SUMS attached to $tag?)"

mv "$work/cliban.rb" "$work/tap/Formula/cliban.rb"

if git -C "$work/tap" diff --quiet -- Formula/cliban.rb; then
	printf 'tap already at %s, nothing to push\n' "$version"
	exit 0
fi

git -C "$work/tap" add Formula/cliban.rb
git -C "$work/tap" commit -q -m "cliban $version"
git -C "$work/tap" push -q origin HEAD

printf 'tap updated to %s\n' "$version"
printf 'verify from a mac: brew update && brew reinstall cliban && cliban --version\n'
