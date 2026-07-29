#!/usr/bin/env python3
"""Assert every file that claims a cliban version agrees with Cargo.toml.

Cargo.toml's [workspace.package] version is canonical. Everything else — the
workspace path-dep pins, each member manifest, Cargo.lock, the AUR PKGBUILD —
has to follow it. Drift here is invisible until a release ships a binary that
reports the wrong version, which is exactly what happened to v0.1.0.

Run by `cook version-sync`; gates `cook release`.
"""

import pathlib
import re
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent


def main() -> int:
    cargo = tomllib.loads((ROOT / "Cargo.toml").read_text())
    canonical = cargo["workspace"]["package"]["version"]
    problems = []

    # The path deps carry an explicit version so `cargo publish` accepts them.
    for name, spec in cargo["workspace"]["dependencies"].items():
        if not (isinstance(spec, dict) and "path" in spec):
            continue
        if spec.get("version") != canonical:
            problems.append(
                f"Cargo.toml [workspace.dependencies] {name} pins "
                f"version = {spec.get('version')!r}, expected {canonical!r}"
            )

    # Members must inherit rather than restate, or bumping misses one.
    for member in cargo["workspace"]["members"]:
        pkg = tomllib.loads((ROOT / member / "Cargo.toml").read_text())["package"]
        if pkg.get("version") != {"workspace": True}:
            problems.append(
                f"{member}/Cargo.toml sets version = {pkg.get('version')!r}; "
                f"use `version.workspace = true`"
            )

    # A stale lock silently ships the old version in the built binary.
    lock = tomllib.loads((ROOT / "Cargo.lock").read_text())
    for pkg in lock["package"]:
        local = pkg.get("source") is None
        if local and pkg["name"].startswith("cliban") and pkg["version"] != canonical:
            problems.append(
                f"Cargo.lock has {pkg['name']} {pkg['version']}, "
                f"expected {canonical} (run: cargo check --workspace)"
            )

    pkgbuild = (ROOT / "packaging/aur/PKGBUILD").read_text()
    found = re.search(r"^pkgver=(.+)$", pkgbuild, re.M)
    if not found:
        problems.append("packaging/aur/PKGBUILD has no pkgver= line")
    elif found.group(1).strip() != canonical:
        problems.append(
            f"packaging/aur/PKGBUILD pkgver={found.group(1).strip()}, "
            f"expected {canonical}"
        )

    if problems:
        print(f"version drift — Cargo.toml says {canonical}:", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        print("\nrun: cook bump " + canonical, file=sys.stderr)
        return 1

    print(f"version {canonical} consistent across every claim site")
    return 0


if __name__ == "__main__":
    sys.exit(main())
