# Packaging

Downstream package definitions for cliban. Both consume the prebuilt tarballs
from the [releases page](https://github.com/LioraLabs/cliban/releases), so
neither needs a Rust toolchain on the user's machine.

## Homebrew

The tap lives in [LioraLabs/homebrew-tap](https://github.com/LioraLabs/homebrew-tap)
(`Formula/cliban.rb`), not here. On a new release, update `version` and the four
`sha256` values from the release's `SHA256SUMS`, then push:

```sh
brew update && brew upgrade cliban       # what users get
brew install --build-from-source cliban  # smoke-test a formula edit locally
```

## AUR

`aur/PKGBUILD` builds `cliban-bin` for x86_64 and aarch64. It is not published
automatically — pushing to the AUR needs an AUR account whose SSH key is
registered at <https://aur.archlinux.org/account>.

To publish a new version:

```sh
cd packaging/aur
updpkgsums                     # fills sha256sums_* from the release tarballs
makepkg --printsrcinfo > .SRCINFO
makepkg -si                    # optional: build and install locally to test

git clone ssh://aur@aur.archlinux.org/cliban-bin.git /tmp/cliban-bin
cp PKGBUILD .SRCINFO /tmp/cliban-bin/
cd /tmp/cliban-bin && git commit -am "upgpkg: cliban-bin 0.2.1-1" && git push
```

`.SRCINFO` is generated, so it is not tracked here — the AUR repo is the only
place it needs to exist.

## Checksums

Every release ships a `SHA256SUMS` file. To pull the values for a version:

```sh
curl -fsSL https://github.com/LioraLabs/cliban/releases/download/v0.2.1/SHA256SUMS
```
