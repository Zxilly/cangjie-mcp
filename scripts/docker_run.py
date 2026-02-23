#!/usr/bin/env python3
"""Run the cangjie-mcp Docker image with runtime env vars from .env."""

import os
import subprocess
import sys
from pathlib import Path

PROJECT_DIR = Path(__file__).resolve().parent.parent
IMAGE_NAME = "cangjie-mcp"

# Required runtime variables
REQUIRED_VARS = ["OPENAI_API_KEY"]

# Forced values — must match the pre-built index in the Docker image
FORCED_VARS = {
    "CANGJIE_EMBEDDING_TYPE": "openai",
    "CANGJIE_RERANK_TYPE": "openai",
}

# Optional runtime env vars the server accepts
OPTIONAL_VARS = [
    "CANGJIE_DOCS_VERSION",
    "CANGJIE_DOCS_LANG",
    "CANGJIE_CHUNK_MAX_SIZE",
    "CANGJIE_RERANK_MODEL",
    "CANGJIE_RERANK_TOP_K",
    "CANGJIE_RERANK_INITIAL_K",
    "CANGJIE_RRF_K",
    "CANGJIE_SERVER_PORT",
    "CANGJIE_DEBUG",
    "OPENAI_API_KEY",
    "OPENAI_BASE_URL",
    "OPENAI_EMBEDDING_MODEL",
]

DEFAULT_PORT = "8765"


def load_env(path: Path) -> dict[str, str]:
    env = {}
    if not path.exists():
        return env
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        key, _, value = line.partition("=")
        env[key.strip()] = value.strip()
    return env


def main():
    env_path = PROJECT_DIR / ".env"
    if not env_path.exists():
        print("ERROR: .env file not found.", file=sys.stderr)
        print(f"  Create {env_path} with at least: OPENAI_API_KEY=sk-xxx", file=sys.stderr)
        print("  See .env.example for a full template.", file=sys.stderr)
        sys.exit(1)

    env = load_env(env_path)

    # Validate required variables
    missing = [v for v in REQUIRED_VARS if not (os.environ.get(v) or env.get(v))]
    if missing:
        print(f"ERROR: Missing required variables: {', '.join(missing)}", file=sys.stderr)
        print("  Set them in .env or as environment variables.", file=sys.stderr)
        sys.exit(1)

    # Warn if user tries to override forced values
    for var, expected in FORCED_VARS.items():
        user_val = os.environ.get(var) or env.get(var)
        if user_val and user_val != expected:
            print(
                f"WARNING: {var} must be '{expected}' to match the pre-built index, "
                f"ignoring '{user_val}'.",
                file=sys.stderr,
            )

    port = os.environ.get("CANGJIE_SERVER_PORT") or env.get("CANGJIE_SERVER_PORT") or DEFAULT_PORT

    cmd = ["docker", "run", "--rm", "-p", f"{port}:{port}"]

    # Pass forced values (always override)
    for var, value in FORCED_VARS.items():
        cmd += ["-e", f"{var}={value}"]

    # Pass optional values from env or .env
    for var in OPTIONAL_VARS:
        value = os.environ.get(var) or env.get(var)
        if value:
            cmd += ["-e", f"{var}={value}"]

    cmd.append(IMAGE_NAME)

    print(f"+ {' '.join(cmd)}")
    sys.exit(subprocess.call(cmd, cwd=PROJECT_DIR))


if __name__ == "__main__":
    main()
