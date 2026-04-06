//! Integration tests for `GitDocumentSource` against the real cangjie_docs repository.
//!
//! These tests require network access.

use cangjie_core::config::DocLang;
use cangjie_indexer::document::source::{DocumentSource, GitDocumentSource};
use cangjie_indexer::repo::GitManager;
use tempfile::TempDir;

async fn setup_repo() -> (TempDir, std::path::PathBuf) {
    let tmp = TempDir::new().unwrap();
    let repo_dir = tmp.path().join("docs_repo");
    let mut mgr = GitManager::new(
        repo_dir.clone(),
        cangjie_core::config::DOCS_REPO_URL.to_string(),
    );
    mgr.ensure_cloned(false).await.unwrap();
    (tmp, repo_dir)
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

    // Every doc should have non-empty text and metadata
    for doc in &docs {
        assert!(!doc.text.is_empty(), "doc text should not be empty");
        assert!(!doc.metadata.category.is_empty());
        assert!(!doc.metadata.topic.is_empty());
        assert!(!doc.metadata.file_path.is_empty());
    }
}
