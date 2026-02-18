#!/usr/bin/env python3
"""Run the cangjie-mcp Docker container locally.

Reads OPENAI_API_KEY from .env and starts the container with port mapping.
"""

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
IMAGE = "cangjie-mcp"
CONTAINER = "cangjie-mcp"
PORT = 8765


def load_env(path: Path) -> dict[str, str]:
    env: dict[str, str] = {}
    with path.open() as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            key, _, value = line.partition("=")
            if value:
                env[key.strip()] = value.strip()
    return env


def main() -> None:
    env = load_env(ROOT / ".env")

    api_key = env.get("OPENAI_API_KEY")
    if not api_key:
        print("OPENAI_API_KEY not found in .env", file=sys.stderr)
        sys.exit(1)

    cmd = [
        "docker",
        "run",
        "--rm",
        "--name",
        CONTAINER,
        "-p",
        f"{PORT}:{PORT}",
        "-e",
        f"OPENAI_API_KEY={api_key}",
        IMAGE,
    ]

    print(f"$ {' '.join(cmd)}")
    raise SystemExit(subprocess.call(cmd, cwd=ROOT))


if __name__ == "__main__":
    main()
