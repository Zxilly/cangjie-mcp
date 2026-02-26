#!/usr/bin/env python3
"""Build and publish the cangjie-mcp Docker image to Alibaba Cloud Container Registry."""

import os
import subprocess
import sys
from pathlib import Path

PROJECT_DIR = Path(__file__).resolve().parent.parent
REGISTRY = "crpi-5ufw1wl9cvjiiv1a.ap-northeast-1.personal.cr.aliyuncs.com"
REPO = f"{REGISTRY}/zxilly/cangjie_mcp"

# Build-time ARGs defined in Dockerfile
BUILD_ARGS = [
    "CANGJIE_DOCS_VERSION",
    "CANGJIE_DOCS_LANG",
    "OPENAI_EMBEDDING_MODEL",
    "OPENAI_BASE_URL",
]

# Build-time secrets
BUILD_SECRETS = ["OPENAI_API_KEY"]

# Required variables
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


CANGJIE_DOCS_REPO = "https://gitcode.com/Cangjie/cangjie_docs.git"


def resolve_cangjie_version(docs_version: str) -> str:
    """Resolve CANGJIE_DOCS_VERSION against the remote cangjie_docs repo.

    Mirrors the logic of resolve_after_checkout in repo/mod.rs:
      - "latest" → resolve main/master branch → "main(<short_hash>)"
      - tag name → tag name as-is (e.g. "v0.55.3")
      - branch name → "branch(<short_hash>)"
    """
    # Fetch all refs from remote
    result = subprocess.run(
        ["git", "ls-remote", CANGJIE_DOCS_REPO],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        print(f"ERROR: Failed to query cangjie_docs remote: {result.stderr}", file=sys.stderr)
        sys.exit(1)

    refs: dict[str, str] = {}
    for line in result.stdout.strip().splitlines():
        oid, ref = line.split("\t", 1)
        refs[ref] = oid

    if docs_version == "latest":
        # Same as Rust: try main, then master
        for branch in ("main", "master"):
            ref_key = f"refs/heads/{branch}"
            if ref_key in refs:
                short = refs[ref_key][:7]
                return f"{branch}({short})"
        print("ERROR: Could not resolve 'latest' — no main/master branch in cangjie_docs.", file=sys.stderr)
        sys.exit(1)

    # Try as tag
    tag_ref = f"refs/tags/{docs_version}"
    if tag_ref in refs:
        return docs_version

    # Try as branch
    branch_ref = f"refs/heads/{docs_version}"
    if branch_ref in refs:
        short = refs[branch_ref][:7]
        return f"{docs_version}({short})"

    print(f"ERROR: Could not resolve CANGJIE_DOCS_VERSION='{docs_version}' as tag or branch in cangjie_docs.", file=sys.stderr)
    sys.exit(1)


def run(cmd: list[str], **kwargs) -> None:
    print(f"+ {' '.join(cmd)}")
    result = subprocess.run(cmd, **kwargs)
    if result.returncode != 0:
        sys.exit(result.returncode)


def git_is_dirty() -> bool:
    result = subprocess.run(
        ["git", "status", "--porcelain"],
        cwd=PROJECT_DIR,
        capture_output=True,
        text=True,
    )
    return bool(result.stdout.strip())


def git_tag() -> str | None:
    result = subprocess.run(
        ["git", "describe", "--tags", "--exact-match", "HEAD"],
        cwd=PROJECT_DIR,
        capture_output=True,
        text=True,
    )
    if result.returncode == 0:
        return result.stdout.strip()
    return None


def git_branch() -> str:
    result = subprocess.run(
        ["git", "rev-parse", "--abbrev-ref", "HEAD"],
        cwd=PROJECT_DIR,
        capture_output=True,
        text=True,
    )
    branch = result.stdout.strip()
    # detached HEAD returns "HEAD"
    return branch if branch != "HEAD" else ""


def git_commit_hash() -> str:
    result = subprocess.run(
        ["git", "rev-parse", "--short", "HEAD"],
        cwd=PROJECT_DIR,
        capture_output=True,
        text=True,
    )
    return result.stdout.strip()


def main():
    # Check git dirty
    if git_is_dirty():
        print("ERROR: Git working tree is dirty. Commit or stash changes before publishing.", file=sys.stderr)
        sys.exit(1)

    # Load .env
    env_path = PROJECT_DIR / ".env"
    if not env_path.exists():
        print("ERROR: .env file not found.", file=sys.stderr)
        print(f"  Create {env_path} with at least: OPENAI_API_KEY=sk-xxx", file=sys.stderr)
        sys.exit(1)

    env = load_env(env_path)

    # Validate required variables
    missing = [v for v in REQUIRED_VARS if not (os.environ.get(v) or env.get(v))]
    if missing:
        print(f"ERROR: Missing required variables: {', '.join(missing)}", file=sys.stderr)
        sys.exit(1)

    # Determine version identifier using the same logic as resolve_after_checkout:
    #   - tag on HEAD → tag name (e.g. "v0.3.0")
    #   - on a branch → "branch-short_hash" (e.g. "main-abc1234")
    #   - detached, no tag → short_hash (e.g. "abc1234")
    tag = git_tag()
    short_hash = git_commit_hash()
    if tag:
        version = tag
    else:
        branch = git_branch()
        version = f"{branch}-{short_hash}" if branch else short_hash

    # Read Cangjie docs version and embedding model for tag suffix
    cangjie_docs_input = os.environ.get("CANGJIE_DOCS_VERSION") or env.get("CANGJIE_DOCS_VERSION", "latest")
    cangjie_version = resolve_cangjie_version(cangjie_docs_input)
    print(f"Resolved Cangjie docs version: {cangjie_docs_input} -> {cangjie_version}")
    embedding_model = os.environ.get("OPENAI_EMBEDDING_MODEL") or env.get("OPENAI_EMBEDDING_MODEL", "BAAI/bge-m3")

    # Sanitize for Docker tag: replace illegal chars ( ) / with Docker-safe equivalents
    cangjie_version_tag = cangjie_version.replace("(", "-").replace(")", "")
    embedding_model_tag = embedding_model.replace("/", "-")

    # Compose image tag: <version>_cj_<cangjie_version>_<embedding_model>
    # Parts separated by "_" to avoid confusion with "-" inside values and "." in versions
    image_tag = f"{version}_cj_{cangjie_version_tag}_{embedding_model_tag}"
    full_image = f"{REPO}:{image_tag}"

    print(f"Publishing: {full_image}")

    # Build
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

    cmd += ["-t", full_image, "."]
    run(cmd, cwd=PROJECT_DIR)

    # Push
    run(["docker", "push", full_image], cwd=PROJECT_DIR)

    print(f"\nPublished: {full_image}")


if __name__ == "__main__":
    main()
