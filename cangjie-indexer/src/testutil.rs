//! Shared test helpers for cangjie-indexer tests.
//!
//! Provides common git repository setup utilities used across multiple test modules.

use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

/// Git commit helper: runs `git init`, `git add .`, `git commit` in the given directory.
pub fn git_init_and_commit(dir: &Path) {
    let run = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "test")
            .env("GIT_AUTHOR_EMAIL", "test@test.com")
            .env("GIT_COMMITTER_NAME", "test")
            .env("GIT_COMMITTER_EMAIL", "test@test.com")
            .status()
            .unwrap();
    };
    run(&["init"]);
    run(&["add", "."]);
    run(&["commit", "-m", "initial commit"]);
}

/// Create a test git repo with the standard cangjie doc structure.
///
/// Structure:
/// ```text
/// docs/dev-guide/source_zh_cn/
///   syntax/
///     functions.md   - "# Functions\n\nContent about functions."
///     variables.md   - "# Variables\n\nContent about variables."
///   stdlib/
///     collections.md - "# Collections\n\nContent about collections."
///   _hidden/
///     secret.md      - "# Secret"
///   .dotdir/
///     hidden.md      - "# Hidden"
///   readme.md        - "# Readme\n\nTop-level readme."
/// ```
pub fn create_test_repo() -> (TempDir, gix::Repository) {
    let tmp = TempDir::new().unwrap();

    let base = tmp
        .path()
        .join("docs")
        .join("dev-guide")
        .join("source_zh_cn");

    let syntax_dir = base.join("syntax");
    std::fs::create_dir_all(&syntax_dir).unwrap();
    std::fs::write(
        syntax_dir.join("functions.md"),
        "# Functions\n\nContent about functions.",
    )
    .unwrap();
    std::fs::write(
        syntax_dir.join("variables.md"),
        "# Variables\n\nContent about variables.",
    )
    .unwrap();

    let stdlib_dir = base.join("stdlib");
    std::fs::create_dir_all(&stdlib_dir).unwrap();
    std::fs::write(
        stdlib_dir.join("collections.md"),
        "# Collections\n\nContent about collections.",
    )
    .unwrap();

    let hidden = base.join("_hidden");
    std::fs::create_dir_all(&hidden).unwrap();
    std::fs::write(hidden.join("secret.md"), "# Secret").unwrap();

    let dotdir = base.join(".dotdir");
    std::fs::create_dir_all(&dotdir).unwrap();
    std::fs::write(dotdir.join("hidden.md"), "# Hidden").unwrap();

    std::fs::write(base.join("readme.md"), "# Readme\n\nTop-level readme.").unwrap();

    git_init_and_commit(tmp.path());

    let repo = gix::open(tmp.path()).unwrap();
    (tmp, repo)
}

/// Create a test repo and add a fake remote with a tracking ref for the given branch name.
pub fn create_test_repo_with_remote(branch: &str) -> (TempDir, gix::Repository) {
    let (tmp, _) = create_test_repo();
    add_fake_remote(&tmp, branch);
    let repo = gix::open(tmp.path()).unwrap();
    (tmp, repo)
}

/// Add a fake remote "origin" and create `refs/remotes/origin/<branch>` pointing at HEAD.
pub fn add_fake_remote(tmp: &TempDir, branch: &str) {
    // Ignore error if remote already exists
    Command::new("git")
        .args(["remote", "add", "origin", "https://example.com/fake.git"])
        .current_dir(tmp.path())
        .status()
        .ok();

    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    let commit_hash = String::from_utf8(output.stdout).unwrap().trim().to_string();

    Command::new("git")
        .args([
            "update-ref",
            &format!("refs/remotes/origin/{branch}"),
            &commit_hash,
        ])
        .current_dir(tmp.path())
        .status()
        .unwrap();
}
