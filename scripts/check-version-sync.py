#!/usr/bin/env python3
"""Assert every file that claims a cliban version agrees with Cargo.toml.

Cargo.toml's [workspace.package] version is canonical. Everything else — the
workspace path-dep pins, each member manifest, Cargo.lock, the AUR PKGBUILD —
has to follow it. Drift here is invisible until a release ships a binary that
reports the wrong version, which is exactly what happened to v0.1.0.

Run by `cook version-sync`; gates `cook release`.
"""

import json
import pathlib
import re
import subprocess
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent

# The two Claude plugins version independently of the crate. The invariant is
# not that their numbers match Cargo.toml — it is that a plugin whose tree
# changed since the last release also changed its version claim, so installed
# copies get a signal they are behind.
PLUGINS = ["plugin", "plugin-flow"]


def git(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["git", "-C", str(ROOT), *args], capture_output=True, text=True, check=False
    )


def plugin_drift() -> list[str]:
    tag = git("describe", "--tags", "--abbrev=0", "--match", "v*").stdout.strip()
    if not tag:
        return []  # no release yet, nothing to drift from
    problems = []
    for plugin in PLUGINS:
        manifest = f"{plugin}/.claude-plugin/plugin.json"
        released = git("show", f"{tag}:{manifest}")
        if released.returncode != 0:
            continue  # plugin did not exist at the last release
        old_version = json.loads(released.stdout).get("version")
        current_path = ROOT / manifest
        if not current_path.exists():
            problems.append(f"{manifest} is missing but existed at {tag}")
            continue
        new_version = json.loads(current_path.read_text()).get("version")
        changed = git("diff", "--quiet", f"{tag}..HEAD", "--", plugin).returncode != 0
        if changed and new_version == old_version:
            problems.append(
                f"{plugin}/ changed since {tag} but {manifest} still claims "
                f"{new_version!r} — bump the plugin version"
            )
    return problems


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

    problems += plugin_drift()

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
