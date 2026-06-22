#!/usr/bin/env python3
"""Build and publish the cangjie-mcp Docker image to Alibaba Cloud Container Registry."""

import hashlib
import os
import subprocess
import sys
from pathlib import Path

PROJECT_DIR = Path(__file__).resolve().parent.parent
REGISTRY = "crpi-5ufw1wl9cvjiiv1a.ap-northeast-1.personal.cr.aliyuncs.com"
REPO = f"{REGISTRY}/zxilly/cangjie_mcp"

# Version inputs — resolved to exact checkout refs before the build so the index
# built in the container is pinned to the same commit recorded in the OCI labels.
VERSION_BUILD_ARGS = [
    "CANGJIE_DOCS_VERSION",
    "CANGJIE_RUNTIME_VERSION",
    "CANGJIE_STDX_VERSION",
]

# Other build-time ARGs passed through from .env / environment (raw user inputs).
OTHER_BUILD_ARGS = [
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


CANGJIE_REPOS = {
    "docs": "https://gitcode.com/Cangjie/cangjie_docs.git",
    "runtime": "https://gitcode.com/Cangjie/cangjie_runtime.git",
    "stdx": "https://gitcode.com/Cangjie/cangjie_stdx.git",
}


def resolve_repo_version(repo_label: str, repo_url: str, version: str) -> tuple[str, str]:
    """Resolve a version against a remote repo.

    Returns ``(display, checkout_ref)``:
      - ``display`` is the human-readable version recorded in the OCI labels and
        the image-tag hash. Mirrors resolve_after_checkout in repo/mod.rs:
          "latest" → "main(<short>)"; tag → tag name; branch → "branch(<short>)".
      - ``checkout_ref`` is a ref the in-container ``cangjie-mcp index`` checks out
        to EXACTLY the commit ``display`` refers to: the tag name for tags, or the
        full commit oid for latest/branch. Passing the oid (not "latest"/branch)
        pins the indexed content to the resolved commit, so it can't drift from
        the labelled version if the branch advances during the build.
    """
    result = subprocess.run(
        ["git", "ls-remote", repo_url],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        print(f"ERROR: Failed to query {repo_label} remote: {result.stderr}", file=sys.stderr)
        sys.exit(1)

    refs: dict[str, str] = {}
    for line in result.stdout.strip().splitlines():
        oid, ref = line.split("\t", 1)
        refs[ref] = oid

    if version == "latest":
        for branch in ("main", "master"):
            ref_key = f"refs/heads/{branch}"
            if ref_key in refs:
                oid = refs[ref_key]
                return f"{branch}({oid[:7]})", oid
        print(f"ERROR: Could not resolve 'latest' — no main/master branch in {repo_label}.", file=sys.stderr)
        sys.exit(1)

    tag_ref = f"refs/tags/{version}"
    if tag_ref in refs:
        return version, version

    branch_ref = f"refs/heads/{version}"
    if branch_ref in refs:
        oid = refs[branch_ref]
        return f"{version}({oid[:7]})", oid

    print(f"ERROR: Could not resolve version='{version}' as tag or branch in {repo_label}.", file=sys.stderr)
    sys.exit(1)


def compute_versions_hash(docs: str, runtime: str, stdx: str) -> str:
    """Stable 12-char hex digest of the three resolved repo versions."""
    payload = f"docs={docs}|runtime={runtime}|stdx={stdx}".encode("utf-8")
    return hashlib.sha256(payload).hexdigest()[:12]


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

    # Resolve docs/runtime/stdx versions; runtime/stdx default to docs version (matches CLI behavior)
    docs_input = os.environ.get("CANGJIE_DOCS_VERSION") or env.get("CANGJIE_DOCS_VERSION", "latest")
    runtime_input = os.environ.get("CANGJIE_RUNTIME_VERSION") or env.get("CANGJIE_RUNTIME_VERSION") or docs_input
    stdx_input = os.environ.get("CANGJIE_STDX_VERSION") or env.get("CANGJIE_STDX_VERSION") or docs_input

    docs_version, docs_ref = resolve_repo_version("docs", CANGJIE_REPOS["docs"], docs_input)
    runtime_version, runtime_ref = resolve_repo_version("runtime", CANGJIE_REPOS["runtime"], runtime_input)
    stdx_version, stdx_ref = resolve_repo_version("stdx", CANGJIE_REPOS["stdx"], stdx_input)
    print(f"Resolved docs:    {docs_input} -> {docs_version} (checkout {docs_ref})")
    print(f"Resolved runtime: {runtime_input} -> {runtime_version} (checkout {runtime_ref})")
    print(f"Resolved stdx:    {stdx_input} -> {stdx_version} (checkout {stdx_ref})")

    versions_hash = compute_versions_hash(docs_version, runtime_version, stdx_version)
    print(f"Versions hash: verhash{versions_hash}")

    embedding_model = os.environ.get("OPENAI_EMBEDDING_MODEL") or env.get("OPENAI_EMBEDDING_MODEL", "BAAI/bge-m3")
    embedding_model_tag = embedding_model.replace("/", "-")

    # Compose image tag: <version>_<embedding_model>_verhash<hash>
    # Three repo versions go to OCI labels; hash makes the version triple addressable from the tag.
    image_tag = f"{version}_{embedding_model_tag}_verhash{versions_hash}"
    full_image = f"{REPO}:{image_tag}"

    print(f"Publishing: {full_image}")

    # Build. The three CANGJIE_*_VERSION args are the RESOLVED checkout refs (tag
    # name, or the exact commit oid for latest/branch), so `cangjie-mcp index`
    # inside the container indexes the same commit recorded in the OCI labels below
    # — it can't drift even if a branch advances mid-build. Other args (lang,
    # embedding model, base url) pass through from .env / environment as raw inputs.
    cmd = ["docker", "build"]

    resolved_versions = {
        "CANGJIE_DOCS_VERSION": docs_ref,
        "CANGJIE_RUNTIME_VERSION": runtime_ref,
        "CANGJIE_STDX_VERSION": stdx_ref,
    }
    for key in VERSION_BUILD_ARGS:
        cmd += ["--build-arg", f"{key}={resolved_versions[key]}"]

    for arg in OTHER_BUILD_ARGS:
        value = os.environ.get(arg) or env.get(arg)
        if value:
            cmd += ["--build-arg", f"{arg}={value}"]

    labels = {
        "org.cangjie-mcp.docs-version": docs_version,
        "org.cangjie-mcp.runtime-version": runtime_version,
        "org.cangjie-mcp.stdx-version": stdx_version,
        "org.cangjie-mcp.versions-hash": versions_hash,
    }
    for key, value in labels.items():
        cmd += ["--label", f"{key}={value}"]

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
