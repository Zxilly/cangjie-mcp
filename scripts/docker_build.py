#!/usr/bin/env python3
"""Build the cangjie-mcp Docker image.

Reads build-arg values and the API key from .env, then invokes
`docker build` with the appropriate --build-arg and --secret flags.
"""

import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
IMAGE = "cangjie-mcp"

BUILD_ARG_KEYS = [
    "OPENAI_BASE_URL",
    "OPENAI_EMBEDDING_MODEL",
    "CANGJIE_DOCS_VERSION",
    "CANGJIE_DOCS_LANG",
    "CANGJIE_RERANK_TYPE",
    "CANGJIE_RERANK_MODEL",
]


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

    cmd = ["docker", "build"]
    for key in BUILD_ARG_KEYS:
        if value := env.get(key):
            cmd += ["--build-arg", f"{key}={value}"]
    cmd += ["--secret", "id=OPENAI_API_KEY,env=OPENAI_API_KEY"]
    cmd += ["-t", IMAGE, "."]

    run_env = os.environ.copy()
    run_env["OPENAI_API_KEY"] = api_key

    print(f"$ {' '.join(cmd)}")
    raise SystemExit(subprocess.call(cmd, cwd=ROOT, env=run_env))


if __name__ == "__main__":
    main()
