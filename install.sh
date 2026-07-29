#!/bin/sh
# cliban installer.
#
#   curl -fsSL https://raw.githubusercontent.com/LioraLabs/cliban/main/install.sh | sh
#
# Downloads the prebuilt cliban + cliband binaries for this platform from the
# GitHub releases page, verifies them against the release SHA256SUMS, and
# installs them into a bin directory on your PATH.
#
# Environment:
#   CLIBAN_VERSION   tag to install (default: the latest release)
#   CLIBAN_BIN_DIR   install directory (default: ~/.local/bin, or /usr/local/bin as root)
#   CLIBAN_NO_DAEMON set to 1 to install only `cliban`, skipping `cliband`

set -eu

REPO="LioraLabs/cliban"
BASE="https://github.com/${REPO}"

say() { printf '%s\n' "$*"; }
err() { printf 'install.sh: %s\n' "$*" >&2; exit 1; }

need() {
	command -v "$1" >/dev/null 2>&1 || err "required command not found: $1"
}

# --- fetch helpers -----------------------------------------------------------

if command -v curl >/dev/null 2>&1; then
	fetch() { curl -fsSL "$1" -o "$2"; }
	fetch_stdout() { curl -fsSL "$1"; }
	resolve_redirect() { curl -fsSLI -o /dev/null -w '%{url_effective}' "$1"; }
elif command -v wget >/dev/null 2>&1; then
	fetch() { wget -qO "$2" "$1"; }
	fetch_stdout() { wget -qO- "$1"; }
	resolve_redirect() {
		wget -q --max-redirect=10 --server-response --spider "$1" 2>&1 |
			awk '/^  Location: /{u=$2} END{print u}'
	}
else
	err "need curl or wget on PATH"
fi

need tar
need uname

if command -v sha256sum >/dev/null 2>&1; then
	sha256() { sha256sum "$1" | cut -d' ' -f1; }
elif command -v shasum >/dev/null 2>&1; then
	sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }
else
	err "need sha256sum or shasum to verify the download"
fi

# --- platform detection ------------------------------------------------------

os=$(uname -s)
arch=$(uname -m)

case "$os" in
	Linux)  os_part="unknown-linux-musl" ;;
	Darwin) os_part="apple-darwin" ;;
	*)      err "unsupported OS: $os (prebuilt binaries cover Linux and macOS; try: cargo install cliban)" ;;
esac

case "$arch" in
	x86_64|amd64)  arch_part="x86_64" ;;
	aarch64|arm64) arch_part="aarch64" ;;
	*)             err "unsupported architecture: $arch (try: cargo install cliban)" ;;
esac

TARGET="${arch_part}-${os_part}"

# --- version resolution ------------------------------------------------------

if [ -n "${CLIBAN_VERSION:-}" ]; then
	VERSION="$CLIBAN_VERSION"
else
	# The /releases/latest URL redirects to /releases/tag/vX.Y.Z. Reading the tag
	# out of the redirect avoids the unauthenticated GitHub API rate limit.
	latest_url=$(resolve_redirect "${BASE}/releases/latest") ||
		err "could not reach GitHub to resolve the latest release"
	VERSION=${latest_url##*/}
	case "$VERSION" in
		v*) ;;
		*)  err "could not parse a version out of '$latest_url'; set CLIBAN_VERSION to pick one manually" ;;
	esac
fi

ARCHIVE="cliban-${VERSION}-${TARGET}.tar.gz"
URL="${BASE}/releases/download/${VERSION}/${ARCHIVE}"

# --- install directory -------------------------------------------------------

if [ -n "${CLIBAN_BIN_DIR:-}" ]; then
	BIN_DIR="$CLIBAN_BIN_DIR"
elif [ "$(id -u)" = 0 ]; then
	BIN_DIR="/usr/local/bin"
else
	BIN_DIR="$HOME/.local/bin"
fi

# --- download, verify, install -----------------------------------------------

tmp=$(mktemp -d 2>/dev/null || mktemp -d -t cliban)
cleanup() { rm -rf "$tmp"; }
trap cleanup EXIT INT TERM

say "cliban ${VERSION} (${TARGET})"

say "  downloading ${ARCHIVE}"
fetch "$URL" "$tmp/$ARCHIVE" ||
	err "download failed: $URL
Check that ${VERSION} exists and ships a ${TARGET} build: ${BASE}/releases"

say "  verifying checksum"
fetch "${BASE}/releases/download/${VERSION}/SHA256SUMS" "$tmp/SHA256SUMS" ||
	err "could not download SHA256SUMS for ${VERSION}"

expected=$(awk -v f="$ARCHIVE" '$2 == f || $2 == "*" f {print $1; exit}' "$tmp/SHA256SUMS")
[ -n "$expected" ] || err "SHA256SUMS for ${VERSION} has no entry for ${ARCHIVE}"

actual=$(sha256 "$tmp/$ARCHIVE")
if [ "$expected" != "$actual" ]; then
	err "checksum mismatch for ${ARCHIVE}
  expected $expected
  actual   $actual
Refusing to install. Please report this at ${BASE}/issues"
fi

tar -xzf "$tmp/$ARCHIVE" -C "$tmp"
unpacked="$tmp/cliban-${VERSION}-${TARGET}"
[ -d "$unpacked" ] || err "unexpected archive layout: $unpacked not found after extraction"

mkdir -p "$BIN_DIR" || err "could not create $BIN_DIR"

installed=""
for bin in cliban cliband; do
	if [ "$bin" = cliband ] && [ "${CLIBAN_NO_DAEMON:-}" = 1 ]; then
		continue
	fi
	[ -f "$unpacked/$bin" ] || err "$bin missing from the archive"
	# Copy to a temp name in the destination and rename, so replacing a running
	# binary can't leave a half-written file behind.
	cp "$unpacked/$bin" "$BIN_DIR/.$bin.new" || err "could not write to $BIN_DIR (try: CLIBAN_BIN_DIR=... or run as root)"
	chmod 755 "$BIN_DIR/.$bin.new"
	mv -f "$BIN_DIR/.$bin.new" "$BIN_DIR/$bin"
	installed="$installed $bin"
done

say "  installed$installed to $BIN_DIR"

# --- PATH check --------------------------------------------------------------

case ":${PATH}:" in
	*":${BIN_DIR}:"*) ;;
	*)
		say ""
		say "$BIN_DIR is not on your PATH. Add it:"
		say ""
		say "  bash/zsh   echo 'export PATH=\"$BIN_DIR:\$PATH\"' >> ~/.profile"
		say "  fish       fish_add_path $BIN_DIR"
		say ""
		;;
esac

say ""
say "Run 'cliban' for the board, or 'cliban --help' for the CLI."
