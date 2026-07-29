#!/usr/bin/env python3
"""Rewrite every version claim in the tree, then refresh Cargo.lock.

    scripts/bump-version.py 0.3.0

Edits are textual and narrow on purpose — a TOML round-trip would reflow the
manifests and bury the real change in whitespace noise. Run by `cook bump`;
`cook version-sync` is what proves it caught everything.
"""

import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SEMVER = re.compile(r"^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$")


def replace_once(path: pathlib.Path, pattern: str, repl: str, label: str) -> bool:
    text = path.read_text()
    new, count = re.subn(pattern, repl, text, flags=re.M)
    if count == 0:
        print(f"  !  {label}: no match for {pattern!r}", file=sys.stderr)
        return False
    if new != text:
        path.write_text(new)
        print(f"  ✓  {label} ({count} site{'s' if count > 1 else ''})")
    else:
        print(f"  =  {label} already current")
    return True


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: bump-version.py <version>   e.g. 0.3.0", file=sys.stderr)
        return 2

    version = sys.argv[1].lstrip("v")
    if not SEMVER.match(version):
        print(f"not a semver version: {version!r}", file=sys.stderr)
        return 2

    print(f"bumping cliban to {version}")
    ok = True

    # [workspace.package] version — the canonical one.
    ok &= replace_once(
        ROOT / "Cargo.toml",
        r'^version = "[^"]+"$',
        f'version = "{version}"',
        "Cargo.toml [workspace.package]",
    )

    # The cliban-* path deps pin an exact version so `cargo publish` accepts them.
    ok &= replace_once(
        ROOT / "Cargo.toml",
        r'^(cliban-[a-z]+ = \{ path = "[^"]+", version = )"[^"]+"',
        rf'\1"{version}"',
        "Cargo.toml [workspace.dependencies]",
    )

    ok &= replace_once(
        ROOT / "packaging/aur/PKGBUILD",
        r"^pkgver=.+$",
        f"pkgver={version}",
        "packaging/aur/PKGBUILD",
    )

    # A version bump resets the package revision.
    ok &= replace_once(
        ROOT / "packaging/aur/PKGBUILD",
        r"^pkgrel=.+$",
        "pkgrel=1",
        "packaging/aur/PKGBUILD pkgrel",
    )

    if not ok:
        print("\nsome sites did not match — fix them before releasing", file=sys.stderr)
        return 1

    print("  …  refreshing Cargo.lock")
    result = subprocess.run(
        ["cargo", "check", "--workspace", "--quiet"],
        cwd=ROOT,
    )
    if result.returncode != 0:
        print("cargo check failed; Cargo.lock may be stale", file=sys.stderr)
        return result.returncode

    print(f"\ncliban is now {version}. Next:")
    print("  cook check                 # prove it")
    print(f"  git commit -am 'chore: v{version}'")
    print(f"  cook release {version}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
