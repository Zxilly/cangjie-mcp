#!/usr/bin/env python3
# /// script
# requires-python = ">=3.9"
# dependencies = ["tomlkit"]
# ///
"""Bump the release version across project manifests.

Usage:
    python scripts/bump_version.py patch       # 0.3.0 -> 0.3.1
    python scripts/bump_version.py minor       # 0.3.0 -> 0.4.0
    python scripts/bump_version.py major       # 0.3.0 -> 1.0.0
    python scripts/bump_version.py 1.2.3       # set explicit version
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

import tomlkit

ROOT = Path(__file__).resolve().parent.parent
EXCLUDED_RUST_MANIFESTS = {
    Path("cangjie-mcp-test/Cargo.toml"),
}
SEMVER_RE = re.compile(r"^\d+\.\d+\.\d+$")
NPM_SEMVER_RE = re.compile(r"^([~^]?)(\d+\.\d+\.\d+)$")


def read_text_with_newline(path: Path) -> tuple[str, str]:
    raw = path.read_bytes()
    newline = "\r\n" if b"\r\n" in raw else "\n"
    text = raw.decode("utf-8").replace("\r\n", "\n")
    return text, newline


def write_text(path: Path, text: str, newline: str = "\n") -> None:
    path.write_bytes(text.replace("\n", newline).encode("utf-8"))


def relative(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def get_rust_release_manifests() -> list[Path]:
    manifests = []
    for path in sorted(ROOT.glob("cangjie-*/Cargo.toml")):
        if path.relative_to(ROOT) in EXCLUDED_RUST_MANIFESTS:
            continue
        manifests.append(path)
    return manifests


def get_npm_package_manifests() -> list[Path]:
    return sorted(ROOT.glob("npm/packages/*/package.json"))


def read_current_version() -> str:
    """Read the current release version from cangjie-core/Cargo.toml."""
    text, _ = read_text_with_newline(ROOT / "cangjie-core/Cargo.toml")
    doc = tomlkit.parse(text)
    version = doc.get("package", {}).get("version")
    if not version:
        sys.exit("ERROR: could not find version in cangjie-core/Cargo.toml")
    return str(version)


def parse_semver(version: str) -> tuple[int, int, int]:
    if not SEMVER_RE.fullmatch(version):
        sys.exit(f"ERROR: invalid semver: {version}")
    major, minor, patch = version.split(".")
    return int(major), int(minor), int(patch)


def compute_new_version(current: str, arg: str) -> str:
    major, minor, patch = parse_semver(current)
    if arg == "patch":
        return f"{major}.{minor}.{patch + 1}"
    if arg == "minor":
        return f"{major}.{minor + 1}.0"
    if arg == "major":
        return f"{major + 1}.0.0"
    parse_semver(arg)
    return arg


def update_toml_version(path: Path, new_version: str) -> bool:
    text, newline = read_text_with_newline(path)
    doc = tomlkit.parse(text)

    table = doc.get("package") or doc.get("project")
    if not table or "version" not in table:
        raise RuntimeError(f"could not find version in {relative(path)}")

    if str(table["version"]) == new_version:
        print(f"  SKIP: {relative(path)} (already {new_version})")
        return False

    table["version"] = new_version
    write_text(path, tomlkit.dumps(doc), newline)
    print(f"  UPDATED: {relative(path)}")
    return True


def update_npm_package(path: Path, new_version: str, internal_packages: set[str]) -> bool:
    text, newline = read_text_with_newline(path)
    data = json.loads(text)
    changed = False

    if data.get("version") != new_version:
        data["version"] = new_version
        changed = True

    for section in ("dependencies", "optionalDependencies", "peerDependencies", "devDependencies"):
        deps = data.get(section)
        if not isinstance(deps, dict):
            continue
        for name in internal_packages:
            if name not in deps:
                continue
            value = str(deps[name])
            match = NPM_SEMVER_RE.fullmatch(value)
            if not match:
                continue
            next_value = f"{match.group(1)}{new_version}"
            if value != next_value:
                deps[name] = next_value
                changed = True

    if not changed:
        print(f"  SKIP: {relative(path)} (already {new_version})")
        return False

    write_text(path, f"{json.dumps(data, indent=2, ensure_ascii=False)}\n", newline)
    print(f"  UPDATED: {relative(path)}")
    return True


def collect_internal_npm_package_names(package_manifests: list[Path]) -> set[str]:
    names = set()
    for path in package_manifests:
        text, _ = read_text_with_newline(path)
        data = json.loads(text)
        name = data.get("name")
        if isinstance(name, str):
            names.add(name)
    return names


def main() -> None:
    if len(sys.argv) != 2 or sys.argv[1] in ("-h", "--help"):
        print(__doc__.strip())
        sys.exit(0 if sys.argv[-1] in ("-h", "--help") else 1)

    rust_manifests = get_rust_release_manifests()
    npm_package_manifests = get_npm_package_manifests()
    internal_npm_packages = collect_internal_npm_package_names(npm_package_manifests)

    current = read_current_version()
    new_version = compute_new_version(current, sys.argv[1])

    print(f"Bumping version: {current} -> {new_version}\n")

    updated = False

    for path in [ROOT / "pyproject.toml", *rust_manifests]:
        if update_toml_version(path, new_version):
            updated = True

    for path in npm_package_manifests:
        if update_npm_package(path, new_version, internal_npm_packages):
            updated = True

    if not updated:
        print("\nNo files were updated.")
        return

    print(f"\nDone. New version: {new_version}")
    print("\nRun the following command to refresh Cargo.lock:")
    print("  cargo check --workspace")


if __name__ == "__main__":
    main()
