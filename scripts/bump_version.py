#!/usr/bin/env python3
"""Bump the version across all project manifests.

Usage:
    python scripts/bump_version.py patch       # 0.3.0 -> 0.3.1
    python scripts/bump_version.py minor       # 0.3.0 -> 0.4.0
    python scripts/bump_version.py major       # 0.3.0 -> 1.0.0
    python scripts/bump_version.py 1.2.3       # set explicit version
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Files to update (relative to project root).
# cangjie-mcp-test is intentionally excluded — it has its own version.
VERSION_FILES = [
    "pyproject.toml",
    "cangjie-mcp/Cargo.toml",
    "cangjie-mcp-cli/Cargo.toml",
    "cangjie-mcp-server/Cargo.toml",
]

VERSION_RE = re.compile(r'^(version\s*=\s*")(\d+\.\d+\.\d+)(")', re.MULTILINE)


def read_current_version() -> str:
    """Read the current version from cangjie-mcp-cli/Cargo.toml (source of truth)."""
    text = (ROOT / "cangjie-mcp-cli/Cargo.toml").read_text(encoding="utf-8")
    m = VERSION_RE.search(text)
    if not m:
        sys.exit("ERROR: could not find version in cangjie-mcp-cli/Cargo.toml")
    return m.group(2)


def parse_semver(version: str) -> tuple[int, int, int]:
    parts = version.split(".")
    if len(parts) != 3 or not all(p.isdigit() for p in parts):
        sys.exit(f"ERROR: invalid semver: {version}")
    return int(parts[0]), int(parts[1]), int(parts[2])


def compute_new_version(current: str, arg: str) -> str:
    major, minor, patch = parse_semver(current)
    if arg == "patch":
        return f"{major}.{minor}.{patch + 1}"
    if arg == "minor":
        return f"{major}.{minor + 1}.0"
    if arg == "major":
        return f"{major + 1}.0.0"
    # Treat as explicit version
    parse_semver(arg)  # validate
    return arg


def update_file(path: Path, new_version: str) -> bool:
    text = path.read_text(encoding="utf-8")
    new_text, count = VERSION_RE.subn(rf"\g<1>{new_version}\3", text, count=1)
    if count == 0:
        print(f"  WARNING: no version found in {path.relative_to(ROOT)}")
        return False
    if new_text == text:
        print(f"  SKIP: {path.relative_to(ROOT)} (already {new_version})")
        return False
    path.write_text(new_text, encoding="utf-8")
    print(f"  UPDATED: {path.relative_to(ROOT)}")
    return True


def main() -> None:
    if len(sys.argv) != 2 or sys.argv[1] in ("-h", "--help"):
        print(__doc__.strip())
        sys.exit(0 if sys.argv[-1] in ("-h", "--help") else 1)

    current = read_current_version()
    new_version = compute_new_version(current, sys.argv[1])

    print(f"Bumping version: {current} -> {new_version}\n")

    updated = False
    for relpath in VERSION_FILES:
        path = ROOT / relpath
        if not path.exists():
            print(f"  WARNING: {relpath} does not exist, skipping")
            continue
        if update_file(path, new_version):
            updated = True

    if not updated:
        print("\nNo files were updated.")
        return

    # Update Cargo.lock to reflect the new versions
    print("\nUpdating Cargo.lock ...")
    result = subprocess.run(
        ["cargo", "check", "--workspace"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        print(f"  WARNING: cargo check failed:\n{result.stderr}")
    else:
        print("  Cargo.lock updated.")

    print(f"\nDone. New version: {new_version}")


if __name__ == "__main__":
    main()
