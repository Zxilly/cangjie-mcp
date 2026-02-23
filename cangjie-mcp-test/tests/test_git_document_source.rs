//! Integration tests for `GitDocumentSource` against the real cangjie_docs repository.
//!
//! These tests require network access (to clone the repo on first run).
//! Run with: `cargo test -p cangjie-mcp-test --test test_git_document_source -- --ignored`

use cangjie_mcp::config::DocLang;
use cangjie_mcp::indexer::document::source::{DocumentSource, GitDocumentSource};
use cangjie_mcp::repo::GitManager;
use tempfile::TempDir;

async fn setup_repo() -> (TempDir, std::path::PathBuf) {
    let tmp = TempDir::new().unwrap();
    let repo_dir = tmp.path().join("docs_repo");
    let mut mgr = GitManager::new(repo_dir.clone());
    mgr.ensure_cloned(false).await.unwrap();
    (tmp, repo_dir)
}

#[tokio::test]
#[ignore]
async fn test_git_source_is_available() {
    let (_tmp, repo_dir) = setup_repo().await;
    let source = GitDocumentSource::new(repo_dir, DocLang::Zh).unwrap();
    assert!(source.is_available().await);
}

#[tokio::test]
#[ignore]
async fn test_git_source_get_categories() {
    let (_tmp, repo_dir) = setup_repo().await;
    let source = GitDocumentSource::new(repo_dir, DocLang::Zh).unwrap();

    let categories = source.get_categories().await.unwrap();
    assert!(!categories.is_empty(), "should have at least one category");
}

#[tokio::test]
#[ignore]
async fn test_git_source_get_topics_in_category() {
    let (_tmp, repo_dir) = setup_repo().await;
    let source = GitDocumentSource::new(repo_dir, DocLang::Zh).unwrap();

    let categories = source.get_categories().await.unwrap();
    assert!(!categories.is_empty());

    let first_cat = &categories[0];
    let topics = source.get_topics_in_category(first_cat).await.unwrap();
    assert!(
        !topics.is_empty(),
        "category '{}' should have topics",
        first_cat
    );
}

#[tokio::test]
#[ignore]
async fn test_git_source_get_document_by_topic() {
    let (_tmp, repo_dir) = setup_repo().await;
    let source = GitDocumentSource::new(repo_dir, DocLang::Zh).unwrap();

    let categories = source.get_categories().await.unwrap();
    let first_cat = &categories[0];
    let topics = source.get_topics_in_category(first_cat).await.unwrap();
    let first_topic = &topics[0];

    let doc = source
        .get_document_by_topic(first_topic, Some(first_cat))
        .await
        .unwrap();
    assert!(
        doc.is_some(),
        "should find document for topic '{}'",
        first_topic
    );

    let doc = doc.unwrap();
    assert!(!doc.text.is_empty(), "document text should not be empty");
    assert_eq!(doc.metadata.category, *first_cat);
    assert_eq!(doc.metadata.topic, *first_topic);
}

#[tokio::test]
#[ignore]
async fn test_git_source_load_all_documents() {
    let (_tmp, repo_dir) = setup_repo().await;
    let source = GitDocumentSource::new(repo_dir, DocLang::Zh).unwrap();

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

#[tokio::test]
#[ignore]
async fn test_git_source_get_all_topic_names() {
    let (_tmp, repo_dir) = setup_repo().await;
    let source = GitDocumentSource::new(repo_dir, DocLang::Zh).unwrap();

    let names = source.get_all_topic_names().await.unwrap();
    assert!(
        names.len() > 5,
        "should have many topic names, got {}",
        names.len()
    );
}

#[tokio::test]
#[ignore]
async fn test_git_source_get_topic_titles() {
    let (_tmp, repo_dir) = setup_repo().await;
    let source = GitDocumentSource::new(repo_dir, DocLang::Zh).unwrap();

    let categories = source.get_categories().await.unwrap();
    let first_cat = &categories[0];
    let titles = source.get_topic_titles(first_cat).await.unwrap();
    assert!(
        !titles.is_empty(),
        "category '{}' should have topic titles",
        first_cat
    );

    // Titles should be non-empty strings
    for (topic, title) in &titles {
        assert!(!topic.is_empty());
        assert!(
            !title.is_empty(),
            "title for topic '{}' should not be empty",
            topic
        );
    }
}

#[tokio::test]
#[ignore]
async fn test_git_source_nonexistent_topic_returns_none() {
    let (_tmp, repo_dir) = setup_repo().await;
    let source = GitDocumentSource::new(repo_dir, DocLang::Zh).unwrap();

    let doc = source
        .get_document_by_topic("nonexistent_topic_xyz", None)
        .await
        .unwrap();
    assert!(doc.is_none());
}

#[tokio::test]
#[ignore]
async fn test_git_source_nonexistent_category_returns_empty() {
    let (_tmp, repo_dir) = setup_repo().await;
    let source = GitDocumentSource::new(repo_dir, DocLang::Zh).unwrap();

    let topics = source
        .get_topics_in_category("nonexistent_category_xyz")
        .await
        .unwrap();
    assert!(topics.is_empty());
}
