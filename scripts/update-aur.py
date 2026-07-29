#!/usr/bin/env python3
"""Refresh the AUR PKGBUILD's checksums from a release's SHA256SUMS.

    scripts/update-aur.py 0.3.0

Deliberately not `updpkgsums`: that shells out to makepkg, which only computes
sums for the running architecture, so it would leave the other arch's line
stale. Reading the release's own SHA256SUMS fills both and needs no Arch tooling,
so this runs anywhere. Run by `cook aur`.
"""

import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
PKGBUILD = ROOT / "packaging/aur/PKGBUILD"
ARCHES = {"x86_64": "x86_64-unknown-linux-musl", "aarch64": "aarch64-unknown-linux-musl"}


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: update-aur.py <version>   e.g. 0.3.0", file=sys.stderr)
        return 2

    version = sys.argv[1].lstrip("v")
    tag = f"v{version}"
    url = f"https://github.com/LioraLabs/cliban/releases/download/{tag}/SHA256SUMS"

    fetched = subprocess.run(
        ["curl", "-fsSL", url], capture_output=True, text=True
    )
    if fetched.returncode != 0:
        print(f"could not fetch {url}\nis the {tag} release published?", file=sys.stderr)
        return 1

    sums = {}
    for line in fetched.stdout.splitlines():
        digest, name = line.split()
        sums[name.lstrip("*")] = digest

    text = PKGBUILD.read_text()

    if (current := re.search(r"^pkgver=(.+)$", text, re.M)) is None:
        print("PKGBUILD has no pkgver= line", file=sys.stderr)
        return 1
    if current.group(1).strip() != version:
        print(
            f"PKGBUILD pkgver is {current.group(1).strip()}, not {version} — "
            f"run `cook bump {version}` first",
            file=sys.stderr,
        )
        return 1

    for arch, target in ARCHES.items():
        archive = f"cliban-{tag}-{target}.tar.gz"
        if archive not in sums:
            print(f"{archive} missing from {tag} SHA256SUMS", file=sys.stderr)
            return 1
        text, count = re.subn(
            rf"^sha256sums_{arch}=\(.*\)$",
            f"sha256sums_{arch}=('{sums[archive]}')",
            text,
            flags=re.M,
        )
        if count != 1:
            print(f"no sha256sums_{arch}= line in PKGBUILD", file=sys.stderr)
            return 1
        print(f"  ✓  sha256sums_{arch}")

    PKGBUILD.write_text(text)
    print(f"\nPKGBUILD is at {version} with real checksums.")
    print("To publish (needs an AUR account with your SSH key registered):")
    print("  cd packaging/aur && makepkg --printsrcinfo > .SRCINFO")
    print("  git clone ssh://aur@aur.archlinux.org/cliban-bin.git /tmp/cliban-bin")
    print("  cp PKGBUILD .SRCINFO /tmp/cliban-bin/ && cd /tmp/cliban-bin")
    print(f"  git commit -am 'upgpkg: cliban-bin {version}-1' && git push")
    return 0


if __name__ == "__main__":
    sys.exit(main())
