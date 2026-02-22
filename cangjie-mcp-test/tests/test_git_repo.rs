//! Integration tests for `repo::GitManager` against the real cangjie_docs repository.
//!
//! These tests require network access and are marked `#[ignore]`.
//! Run with: `cargo test -p cangjie-mcp-test --test test_git_repo -- --ignored`

use cangjie_mcp::repo::GitManager;
use tempfile::TempDir;

fn clone_repo() -> (TempDir, GitManager) {
    let tmp = TempDir::new().unwrap();
    let repo_dir = tmp.path().join("docs_repo");
    let mut mgr = GitManager::new(repo_dir);
    mgr.ensure_cloned(false).unwrap();
    (tmp, mgr)
}

#[test]
#[ignore]
fn test_clone_and_is_cloned() {
    let (_tmp, mgr) = clone_repo();
    assert!(mgr.is_cloned());
    assert!(mgr.repo().is_some());
}

#[test]
#[ignore]
fn test_resolve_version_latest() {
    let tmp = TempDir::new().unwrap();
    let repo_dir = tmp.path().join("docs_repo");
    let mut mgr = GitManager::new(repo_dir);

    let resolved = mgr.resolve_version("latest").unwrap();
    // "latest" resolves to branch(short_hash) format or a tag
    assert!(!resolved.is_empty(), "resolved version should not be empty");
}

#[test]
#[ignore]
fn test_list_tree_dirs_has_docs() {
    let (_tmp, mgr) = clone_repo();

    let top_dirs = mgr.list_tree_dirs("docs").unwrap();
    assert!(
        top_dirs.contains(&"dev-guide".to_string()),
        "docs/ should contain dev-guide, got: {:?}",
        top_dirs
    );
}

#[test]
#[ignore]
fn test_list_tree_dirs_zh_source() {
    let (_tmp, mgr) = clone_repo();

    let categories = mgr.list_tree_dirs("docs/dev-guide/source_zh_cn").unwrap();
    assert!(
        !categories.is_empty(),
        "source_zh_cn should have category directories"
    );
}

#[test]
#[ignore]
fn test_list_md_files() {
    let (_tmp, mgr) = clone_repo();

    let files = mgr.list_md_files("docs/dev-guide/source_zh_cn").unwrap();
    assert!(
        !files.is_empty(),
        "should find .md files in the documentation"
    );
    assert!(
        files.iter().all(|f| f.ends_with(".md")),
        "all files should be .md"
    );
}

#[test]
#[ignore]
fn test_read_file_from_tree() {
    let (_tmp, mgr) = clone_repo();

    // The repo should have a top-level docs directory with some markdown
    let files = mgr.list_md_files("docs/dev-guide/source_zh_cn").unwrap();
    assert!(!files.is_empty());

    let first_file = &files[0];
    let full_path = format!("docs/dev-guide/source_zh_cn/{first_file}");
    let content = mgr.read_file_from_tree(&full_path).unwrap();
    assert!(!content.is_empty(), "file content should not be empty");
}

#[test]
#[ignore]
fn test_checkout_nonexistent_version_fails() {
    let tmp = TempDir::new().unwrap();
    let repo_dir = tmp.path().join("docs_repo");
    let mut mgr = GitManager::new(repo_dir);

    let result = mgr.checkout("nonexistent_version_xyz_12345");
    assert!(
        result.is_err(),
        "checkout of nonexistent version should fail"
    );
}

#[test]
#[ignore]
fn test_ensure_cloned_twice_is_idempotent() {
    let tmp = TempDir::new().unwrap();
    let repo_dir = tmp.path().join("docs_repo");
    let mut mgr = GitManager::new(repo_dir);

    mgr.ensure_cloned(false).unwrap();
    assert!(mgr.is_cloned());

    // Second call should succeed without re-cloning
    mgr.ensure_cloned(false).unwrap();
    assert!(mgr.is_cloned());
}
