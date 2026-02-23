#!/usr/bin/env python3
"""Build the cangjie-mcp Docker image with build args and secrets from .env."""

import os
import subprocess
import sys
from pathlib import Path

PROJECT_DIR = Path(__file__).resolve().parent.parent
IMAGE_NAME = "cangjie-mcp"

# Build-time ARGs defined in Dockerfile (optional overrides)
BUILD_ARGS = [
    "CANGJIE_DOCS_VERSION",
    "CANGJIE_DOCS_LANG",
    "OPENAI_EMBEDDING_MODEL",
    "OPENAI_BASE_URL",
]

# Build-time secrets
BUILD_SECRETS = ["OPENAI_API_KEY"]

# Required variables (must be present in .env or environment)
REQUIRED_VARS = ["OPENAI_API_KEY"]


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

    cmd = ["docker", "build"]

    for arg in BUILD_ARGS:
        value = os.environ.get(arg) or env.get(arg)
        if value:
            cmd += ["--build-arg", f"{arg}={value}"]

    for secret in BUILD_SECRETS:
        value = os.environ.get(secret) or env.get(secret)
        if value:
            cmd += ["--secret", f"id={secret},env={secret}"]
            os.environ[secret] = value

    cmd += ["-t", IMAGE_NAME, "."]

    print(f"+ {' '.join(cmd)}")
    sys.exit(subprocess.call(cmd, cwd=PROJECT_DIR))


if __name__ == "__main__":
    main()
