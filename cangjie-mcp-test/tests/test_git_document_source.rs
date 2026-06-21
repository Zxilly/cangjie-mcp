//! Integration tests for `GitDocumentSource` against the real cangjie_docs repository.
//!
//! These tests require network access.

use cangjie_core::config::DocLang;
use cangjie_indexer::document::source::{DocumentSource, GitDocumentSource};
use cangjie_indexer::repo::GitManager;
use tempfile::TempDir;

async fn clone_into_tmp(subdir: &str, url: &str) -> (TempDir, std::path::PathBuf) {
    let tmp = TempDir::new().unwrap();
    let repo_dir = tmp.path().join(subdir);
    let mut mgr = GitManager::new(repo_dir.clone(), url.to_string());
    mgr.ensure_cloned(false).await.unwrap();
    (tmp, repo_dir)
}

async fn setup_repo() -> (TempDir, std::path::PathBuf) {
    clone_into_tmp("docs_repo", cangjie_core::config::DOCS_REPO_URL).await
}

async fn setup_stdx_repo() -> (TempDir, std::path::PathBuf) {
    clone_into_tmp("stdx_repo", cangjie_core::config::STDX_REPO_URL).await
}

#[tokio::test]
async fn test_git_source_is_available() {
    let (_tmp, repo_dir) = setup_repo().await;
    let source = GitDocumentSource::for_docs(repo_dir, DocLang::Zh).unwrap();
    assert!(source.is_available().await);
}

#[tokio::test]
async fn test_git_source_load_all_documents() {
    let (_tmp, repo_dir) = setup_repo().await;
    let source = GitDocumentSource::for_docs(repo_dir, DocLang::Zh).unwrap();

    let docs = source.load_all_documents().await.unwrap();
    assert!(
        docs.len() > 10,
        "should load a substantial number of documents, got {}",
        docs.len()
    );

    for doc in &docs {
        assert!(!doc.text.is_empty(), "doc text should not be empty");
        assert!(!doc.metadata.category.is_empty());
        assert!(!doc.metadata.topic.is_empty());
        assert!(!doc.metadata.file_path.is_empty());
    }
}

#[tokio::test]
async fn test_git_source_for_tools_loads_docs() {
    let (_tmp, repo_dir) = setup_repo().await;
    let source = GitDocumentSource::for_tools(repo_dir, DocLang::Zh).unwrap();

    let docs = source.load_all_documents().await.unwrap();
    assert!(
        !docs.is_empty(),
        "tools section should have at least one document"
    );

    for doc in &docs {
        assert!(!doc.text.is_empty());
        assert!(
            doc.metadata.category.starts_with("tools"),
            "category should start with 'tools', got {}",
            doc.metadata.category
        );
        assert!(!doc.metadata.topic.is_empty());
    }
}

#[tokio::test]
async fn test_git_source_for_stdx_loads_docs() {
    let (_tmp, repo_dir) = setup_stdx_repo().await;
    let source = GitDocumentSource::for_stdx(repo_dir, DocLang::Zh).unwrap();

    let docs = source.load_all_documents().await.unwrap();
    assert!(
        docs.len() > 10,
        "stdx should load a substantial number of documents, got {}",
        docs.len()
    );

    for doc in &docs {
        assert!(!doc.text.is_empty());
        assert!(
            doc.metadata.category.starts_with("stdx"),
            "category should start with 'stdx', got {}",
            doc.metadata.category
        );
        assert!(!doc.metadata.topic.is_empty());
    }

    // Sanity: at least one of the well-known top-level packages shows up.
    let has_known_pkg = docs.iter().any(|d| {
        matches!(
            d.metadata.category.as_str(),
            "stdx/encoding" | "stdx/crypto" | "stdx/log" | "stdx/net"
        )
    });
    assert!(
        has_known_pkg,
        "expected at least one doc under a known stdx package"
    );
}

#[tokio::test]
async fn test_git_source_for_release_notes_loads_docs() {
    let (_tmp, repo_dir) = setup_repo().await;
    let source = GitDocumentSource::for_release_notes(repo_dir).unwrap();

    let docs = source.load_all_documents().await.unwrap();
    assert!(
        !docs.is_empty(),
        "release-notes should have at least one document"
    );

    for doc in &docs {
        assert!(!doc.text.is_empty());
        assert_eq!(doc.metadata.category, "release-notes");
    }
}
