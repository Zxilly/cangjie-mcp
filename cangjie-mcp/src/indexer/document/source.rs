use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use async_trait::async_trait;
use tracing::{info, warn};

use crate::config::DocLang;
use crate::indexer::document::loader::{extract_title_from_content, load_document_from_content};
use crate::indexer::DocData;

// -- Document Source trait ---------------------------------------------------

#[async_trait]
pub trait DocumentSource: Send + Sync {
    async fn is_available(&self) -> bool;
    async fn get_categories(&self) -> Result<Vec<String>>;
    async fn get_topics_in_category(&self, category: &str) -> Result<Vec<String>>;
    async fn get_document_by_topic(
        &self,
        topic: &str,
        category: Option<&str>,
    ) -> Result<Option<DocData>>;
    async fn load_all_documents(&self) -> Result<Vec<DocData>>;
    async fn get_all_topic_names(&self) -> Result<Vec<String>>;
    async fn get_topic_titles(&self, category: &str) -> Result<HashMap<String, String>>;
}

// -- Sync git2 helpers (run inside spawn_blocking) ---------------------------

fn open_repo(repo_dir: &Path) -> Result<git2::Repository> {
    git2::Repository::open(repo_dir).context("Failed to open git repository")
}

fn read_file(repo_dir: &Path, path: &str) -> Result<String> {
    let repo = open_repo(repo_dir)?;
    let head = repo.head()?;
    let tree = head.peel_to_tree()?;
    let entry = tree
        .get_path(Path::new(path))
        .with_context(|| format!("Path not found: {path}"))?;
    let blob = repo.find_blob(entry.id())?;
    Ok(std::str::from_utf8(blob.content())?.to_string())
}

fn list_dirs(repo_dir: &Path, path: &str) -> Result<Vec<String>> {
    let repo = open_repo(repo_dir)?;
    let head = repo.head()?;
    let tree = head.peel_to_tree()?;
    let entry = tree.get_path(Path::new(path))?;
    let subtree = repo.find_tree(entry.id())?;
    let mut dirs = Vec::new();
    for item in subtree.iter() {
        if item.kind() == Some(git2::ObjectType::Tree) {
            if let Some(name) = item.name() {
                if !name.starts_with('.') && !name.starts_with('_') {
                    dirs.push(name.to_string());
                }
            }
        }
    }
    dirs.sort();
    Ok(dirs)
}

fn list_md_files(repo_dir: &Path, base_path: &str) -> Result<Vec<String>> {
    let repo = open_repo(repo_dir)?;
    let head = repo.head()?;
    let root_tree = head.peel_to_tree()?;
    let entry = root_tree.get_path(Path::new(base_path))?;
    let subtree = repo.find_tree(entry.id())?;
    let mut files = Vec::new();
    collect_md_files_recursive(&repo, &subtree, "", &mut files)?;
    files.sort();
    Ok(files)
}

fn is_git_not_found_error(err: &anyhow::Error) -> bool {
    err.chain()
        .filter_map(|e| e.downcast_ref::<git2::Error>())
        .any(|e| e.code() == git2::ErrorCode::NotFound)
}

/// Convert git tree "path not found" into an empty business result while
/// preserving other infrastructure errors for observability.
fn default_on_git_not_found<T, F>(result: Result<T>, default: F) -> Result<T>
where
    F: FnOnce() -> T,
{
    match result {
        Ok(value) => Ok(value),
        Err(err) if is_git_not_found_error(&err) => Ok(default()),
        Err(err) => Err(err),
    }
}

fn topic_name_from_md_path(path: &str) -> Option<String> {
    path.rsplit('/')
        .next()
        .and_then(|name| name.strip_suffix(".md"))
        .map(ToString::to_string)
}

fn collect_md_files_recursive(
    repo: &git2::Repository,
    tree: &git2::Tree,
    prefix: &str,
    files: &mut Vec<String>,
) -> Result<()> {
    for item in tree.iter() {
        let name = item.name().unwrap_or("");
        let path = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}/{name}")
        };
        match item.kind() {
            Some(git2::ObjectType::Blob) if name.ends_with(".md") => {
                files.push(path);
            }
            Some(git2::ObjectType::Tree) => {
                let subtree = repo.find_tree(item.id())?;
                collect_md_files_recursive(repo, &subtree, &path, files)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn build_topic_index(repo_dir: &Path, docs_base_path: &str) -> Result<HashMap<String, String>> {
    let categories = list_dirs(repo_dir, docs_base_path)?;
    let mut mapping = HashMap::new();
    for cat in &categories {
        let path = format!("{docs_base_path}/{cat}");
        match list_md_files(repo_dir, &path) {
            Ok(files) => {
                for f in &files {
                    if let Some(topic) = topic_name_from_md_path(f) {
                        mapping.entry(topic).or_insert_with(|| cat.clone());
                    }
                }
            }
            Err(err) if is_git_not_found_error(&err) => {}
            Err(err) => {
                warn!(
                    "Failed to list markdown files for category '{}' at '{}': {}",
                    cat, path, err
                );
            }
        }
    }
    info!(
        "Topic index built: {} topics across categories",
        mapping.len()
    );
    Ok(mapping)
}

// -- Git Document Source -----------------------------------------------------

pub struct GitDocumentSource {
    repo_dir: PathBuf,
    docs_base_path: String,
    topic_index: tokio::sync::OnceCell<HashMap<String, String>>,
}

impl GitDocumentSource {
    pub fn new(repo_dir: PathBuf, lang: DocLang) -> Result<Self> {
        let docs_base_path = format!("docs/dev-guide/{}", lang.source_dir_name());

        Ok(Self {
            repo_dir,
            docs_base_path,
            topic_index: tokio::sync::OnceCell::new(),
        })
    }

    async fn get_cached_topic_index(&self) -> &HashMap<String, String> {
        self.topic_index
            .get_or_init(|| async {
                let repo_dir = self.repo_dir.clone();
                let base = self.docs_base_path.clone();
                tokio::task::spawn_blocking(move || {
                    build_topic_index(&repo_dir, &base).unwrap_or_default()
                })
                .await
                .unwrap_or_default()
            })
            .await
    }
}

#[async_trait]
impl DocumentSource for GitDocumentSource {
    async fn is_available(&self) -> bool {
        self.repo_dir.exists() && self.repo_dir.join(".git").exists()
    }

    async fn get_categories(&self) -> Result<Vec<String>> {
        let repo_dir = self.repo_dir.clone();
        let base = self.docs_base_path.clone();
        let categories = tokio::task::spawn_blocking(move || list_dirs(&repo_dir, &base))
            .await
            .context("get_categories task panicked")?;
        default_on_git_not_found(categories, Vec::new)
    }

    async fn get_topics_in_category(&self, category: &str) -> Result<Vec<String>> {
        let repo_dir = self.repo_dir.clone();
        let path = format!("{}/{category}", self.docs_base_path);
        let files_result = tokio::task::spawn_blocking(move || list_md_files(&repo_dir, &path))
            .await
            .context("get_topics_in_category task panicked")?;
        let files = default_on_git_not_found(files_result, Vec::new)?;

        let mut topics: Vec<String> = files
            .iter()
            .filter_map(|f| topic_name_from_md_path(f))
            .collect();
        topics.sort();
        Ok(topics)
    }

    async fn get_document_by_topic(
        &self,
        topic: &str,
        category: Option<&str>,
    ) -> Result<Option<DocData>> {
        let category = match category {
            Some(c) => c.to_string(),
            None => match self.get_cached_topic_index().await.get(topic) {
                Some(c) => c.clone(),
                None => return Ok(None),
            },
        };

        let repo_dir = self.repo_dir.clone();
        let base = self.docs_base_path.clone();
        let topic = topic.to_string();
        let cat = category.clone();

        tokio::task::spawn_blocking(move || {
            let filename = format!("{topic}.md");
            let path = format!("{base}/{cat}");
            let files = default_on_git_not_found(list_md_files(&repo_dir, &path), Vec::new)?;
            for file in &files {
                let file_name = file.rsplit('/').next().unwrap_or("");
                if file_name == filename {
                    let full_path = format!("{path}/{file}");
                    match read_file(&repo_dir, &full_path) {
                        Ok(content) => {
                            let relative_path = format!("{cat}/{file}");
                            return Ok(load_document_from_content(
                                content,
                                &relative_path,
                                &cat,
                                &topic,
                            ));
                        }
                        Err(e) => {
                            warn!("Failed to read {}: {}", full_path, e);
                        }
                    }
                }
            }
            Ok(None)
        })
        .await
        .context("get_document_by_topic task panicked")?
    }

    async fn load_all_documents(&self) -> Result<Vec<DocData>> {
        let repo_dir = self.repo_dir.clone();
        let base = self.docs_base_path.clone();

        tokio::task::spawn_blocking(move || {
            let categories = default_on_git_not_found(list_dirs(&repo_dir, &base), Vec::new)?;
            let mut documents = Vec::new();

            for category in &categories {
                let path = format!("{base}/{category}");
                let files = default_on_git_not_found(list_md_files(&repo_dir, &path), Vec::new)?;

                for file in &files {
                    let full_path = format!("{path}/{file}");
                    match read_file(&repo_dir, &full_path) {
                        Ok(content) => {
                            let topic = topic_name_from_md_path(file).unwrap_or_default();
                            let relative_path = format!("{category}/{file}");
                            if let Some(doc) = load_document_from_content(
                                content,
                                &relative_path,
                                category,
                                &topic,
                            ) {
                                documents.push(doc);
                            }
                        }
                        Err(e) => {
                            warn!("Failed to load {}: {}", full_path, e);
                        }
                    }
                }
            }

            info!("Loaded {} documents from git.", documents.len());
            Ok(documents)
        })
        .await
        .context("load_all_documents task panicked")?
    }

    async fn get_all_topic_names(&self) -> Result<Vec<String>> {
        let index = self.get_cached_topic_index().await;
        let mut names: Vec<String> = index.keys().cloned().collect();
        names.sort();
        Ok(names)
    }

    async fn get_topic_titles(&self, category: &str) -> Result<HashMap<String, String>> {
        let repo_dir = self.repo_dir.clone();
        let base = self.docs_base_path.clone();
        let category = category.to_string();

        tokio::task::spawn_blocking(move || {
            let path = format!("{base}/{category}");
            let files = default_on_git_not_found(list_md_files(&repo_dir, &path), Vec::new)?;
            let mut titles = HashMap::new();

            for file in &files {
                let topic = topic_name_from_md_path(file).unwrap_or_default();
                let full_path = format!("{path}/{file}");
                match read_file(&repo_dir, &full_path) {
                    Ok(content) => {
                        let title = extract_title_from_content(&content);
                        titles.insert(topic, title);
                    }
                    Err(_) => {
                        titles.insert(topic, String::new());
                    }
                }
            }

            Ok(titles)
        })
        .await
        .context("get_topic_titles task panicked")?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{Repository, Signature};
    use tempfile::TempDir;

    /// Create a test git repository with the expected doc structure for GitDocumentSource.
    ///
    /// Structure:
    ///   docs/dev-guide/source_zh_cn/
    ///     syntax/
    ///       functions.md   - "# Functions\n\nContent about functions."
    ///       variables.md   - "# Variables\n\nContent about variables."
    ///     stdlib/
    ///       collections.md - "# Collections\n\nContent about collections."
    ///     _hidden/
    ///       secret.md      - "# Secret"
    fn create_test_repo() -> (TempDir, Repository) {
        let tmp = TempDir::new().unwrap();
        let repo = Repository::init(tmp.path()).unwrap();

        let base = tmp
            .path()
            .join("docs")
            .join("dev-guide")
            .join("source_zh_cn");

        // syntax category
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

        // stdlib category
        let stdlib_dir = base.join("stdlib");
        std::fs::create_dir_all(&stdlib_dir).unwrap();
        std::fs::write(
            stdlib_dir.join("collections.md"),
            "# Collections\n\nContent about collections.",
        )
        .unwrap();

        // hidden dir (should be ignored by list_dirs)
        let hidden = base.join("_hidden");
        std::fs::create_dir_all(&hidden).unwrap();
        std::fs::write(hidden.join("secret.md"), "# Secret").unwrap();

        // Stage and commit
        {
            let mut index = repo.index().unwrap();
            index
                .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
                .unwrap();
            index.write().unwrap();
            let tree_id = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            let sig = Signature::now("test", "test@test.com").unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])
                .unwrap();
        }

        (tmp, repo)
    }

    fn create_test_repo_without_docs_base() -> (TempDir, Repository) {
        let tmp = TempDir::new().unwrap();
        let repo = Repository::init(tmp.path()).unwrap();
        std::fs::write(tmp.path().join("README.md"), "# Placeholder").unwrap();

        {
            let mut index = repo.index().unwrap();
            index
                .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
                .unwrap();
            index.write().unwrap();
            let tree_id = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            let sig = Signature::now("test", "test@test.com").unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])
                .unwrap();
        }

        (tmp, repo)
    }

    #[test]
    fn test_git_source_new() {
        let tmp = TempDir::new().unwrap();
        let source = GitDocumentSource::new(tmp.path().to_path_buf(), DocLang::Zh);
        assert!(source.is_ok());
        let source = source.unwrap();
        assert_eq!(source.docs_base_path, "docs/dev-guide/source_zh_cn");
    }

    #[test]
    fn test_git_source_new_en() {
        let tmp = TempDir::new().unwrap();
        let source = GitDocumentSource::new(tmp.path().to_path_buf(), DocLang::En).unwrap();
        assert_eq!(source.docs_base_path, "docs/dev-guide/source_en");
    }

    #[tokio::test]
    async fn test_git_source_is_available() {
        let (tmp, _repo) = create_test_repo();
        let source = GitDocumentSource::new(tmp.path().to_path_buf(), DocLang::Zh).unwrap();
        assert!(source.is_available().await);
    }

    #[tokio::test]
    async fn test_git_source_not_available() {
        let tmp = TempDir::new().unwrap();
        let nonexistent = tmp.path().join("nonexistent");
        let source = GitDocumentSource::new(nonexistent, DocLang::Zh).unwrap();
        assert!(!source.is_available().await);
    }

    #[tokio::test]
    async fn test_git_source_not_available_no_git_dir() {
        // Directory exists but no .git
        let tmp = TempDir::new().unwrap();
        let source = GitDocumentSource::new(tmp.path().to_path_buf(), DocLang::Zh).unwrap();
        assert!(!source.is_available().await);
    }

    #[tokio::test]
    async fn test_git_source_get_categories() {
        let (tmp, _repo) = create_test_repo();
        let source = GitDocumentSource::new(tmp.path().to_path_buf(), DocLang::Zh).unwrap();

        let categories = source.get_categories().await.unwrap();
        assert!(categories.contains(&"stdlib".to_string()));
        assert!(categories.contains(&"syntax".to_string()));
        // _hidden should be filtered out
        assert!(!categories.contains(&"_hidden".to_string()));
        // Should be sorted
        assert_eq!(categories, vec!["stdlib", "syntax"]);
    }

    #[tokio::test]
    async fn test_git_source_get_categories_without_docs_base_returns_empty() {
        let (tmp, _repo) = create_test_repo_without_docs_base();
        let source = GitDocumentSource::new(tmp.path().to_path_buf(), DocLang::Zh).unwrap();

        let categories = source.get_categories().await.unwrap();
        assert!(categories.is_empty());
    }

    #[tokio::test]
    async fn test_git_source_get_topics_in_category() {
        let (tmp, _repo) = create_test_repo();
        let source = GitDocumentSource::new(tmp.path().to_path_buf(), DocLang::Zh).unwrap();

        let topics = source.get_topics_in_category("syntax").await.unwrap();
        assert!(topics.contains(&"functions".to_string()));
        assert!(topics.contains(&"variables".to_string()));
        assert_eq!(topics.len(), 2);
    }

    #[tokio::test]
    async fn test_git_source_get_topics_in_category_stdlib() {
        let (tmp, _repo) = create_test_repo();
        let source = GitDocumentSource::new(tmp.path().to_path_buf(), DocLang::Zh).unwrap();

        let topics = source.get_topics_in_category("stdlib").await.unwrap();
        assert_eq!(topics, vec!["collections"]);
    }

    #[tokio::test]
    async fn test_git_source_get_document_by_topic() {
        let (tmp, _repo) = create_test_repo();
        let source = GitDocumentSource::new(tmp.path().to_path_buf(), DocLang::Zh).unwrap();

        let doc = source
            .get_document_by_topic("functions", Some("syntax"))
            .await
            .unwrap();
        assert!(doc.is_some());
        let doc = doc.unwrap();
        assert!(doc.text.contains("# Functions"));
        assert!(doc.text.contains("Content about functions."));
        assert_eq!(doc.metadata.category, "syntax");
        assert_eq!(doc.metadata.topic, "functions");
        assert_eq!(doc.metadata.title, "Functions");
    }

    #[tokio::test]
    async fn test_git_source_get_document_not_found() {
        let (tmp, _repo) = create_test_repo();
        let source = GitDocumentSource::new(tmp.path().to_path_buf(), DocLang::Zh).unwrap();

        let doc = source
            .get_document_by_topic("nonexistent", Some("syntax"))
            .await
            .unwrap();
        assert!(doc.is_none());
    }

    #[tokio::test]
    async fn test_git_source_get_document_not_found_in_nonexistent_category() {
        let (tmp, _repo) = create_test_repo();
        let source = GitDocumentSource::new(tmp.path().to_path_buf(), DocLang::Zh).unwrap();

        let doc = source
            .get_document_by_topic("functions", Some("nonexistent_category_xyz"))
            .await
            .unwrap();
        assert!(doc.is_none());
    }

    #[tokio::test]
    async fn test_git_source_get_document_not_found_no_category() {
        let (tmp, _repo) = create_test_repo();
        let source = GitDocumentSource::new(tmp.path().to_path_buf(), DocLang::Zh).unwrap();

        let doc = source
            .get_document_by_topic("totally_nonexistent", None)
            .await
            .unwrap();
        assert!(doc.is_none());
    }

    #[tokio::test]
    async fn test_git_source_load_all_documents() {
        let (tmp, _repo) = create_test_repo();
        let source = GitDocumentSource::new(tmp.path().to_path_buf(), DocLang::Zh).unwrap();

        let docs = source.load_all_documents().await.unwrap();
        // Should load docs from categories visible via list_dirs (which filters _hidden).
        // Categories: stdlib (1 doc), syntax (2 docs) = 3 total
        assert_eq!(docs.len(), 3);

        let topics: Vec<&str> = docs.iter().map(|d| d.metadata.topic.as_str()).collect();
        assert!(topics.contains(&"functions"));
        assert!(topics.contains(&"variables"));
        assert!(topics.contains(&"collections"));
    }

    #[tokio::test]
    async fn test_git_source_get_all_topic_names() {
        let (tmp, _repo) = create_test_repo();
        let source = GitDocumentSource::new(tmp.path().to_path_buf(), DocLang::Zh).unwrap();

        let names = source.get_all_topic_names().await.unwrap();
        assert!(names.contains(&"functions".to_string()));
        assert!(names.contains(&"variables".to_string()));
        assert!(names.contains(&"collections".to_string()));
        // Should be sorted
        assert_eq!(names, {
            let mut sorted = names.clone();
            sorted.sort();
            sorted
        });
    }

    #[tokio::test]
    async fn test_git_source_get_topic_titles() {
        let (tmp, _repo) = create_test_repo();
        let source = GitDocumentSource::new(tmp.path().to_path_buf(), DocLang::Zh).unwrap();

        let titles = source.get_topic_titles("syntax").await.unwrap();
        assert_eq!(titles.get("functions").unwrap(), "Functions");
        assert_eq!(titles.get("variables").unwrap(), "Variables");
        assert_eq!(titles.len(), 2);
    }

    #[tokio::test]
    async fn test_git_source_get_topic_titles_stdlib() {
        let (tmp, _repo) = create_test_repo();
        let source = GitDocumentSource::new(tmp.path().to_path_buf(), DocLang::Zh).unwrap();

        let titles = source.get_topic_titles("stdlib").await.unwrap();
        assert_eq!(titles.get("collections").unwrap(), "Collections");
        assert_eq!(titles.len(), 1);
    }

    #[test]
    fn test_build_topic_index() {
        let (tmp, _repo) = create_test_repo();

        let index = build_topic_index(tmp.path(), "docs/dev-guide/source_zh_cn").unwrap();
        // Should map topic -> category
        assert_eq!(index.get("functions").unwrap(), "syntax");
        assert_eq!(index.get("variables").unwrap(), "syntax");
        assert_eq!(index.get("collections").unwrap(), "stdlib");
        assert_eq!(index.len(), 3);
    }

    #[test]
    fn test_read_file() {
        let (tmp, _repo) = create_test_repo();

        let content = read_file(
            tmp.path(),
            "docs/dev-guide/source_zh_cn/syntax/functions.md",
        )
        .unwrap();
        assert!(content.contains("# Functions"));
        assert!(content.contains("Content about functions."));
    }

    #[test]
    fn test_read_file_not_found() {
        let (tmp, _repo) = create_test_repo();

        let result = read_file(tmp.path(), "nonexistent/file.md");
        assert!(result.is_err());
    }

    #[test]
    fn test_list_dirs() {
        let (tmp, _repo) = create_test_repo();

        let dirs = list_dirs(tmp.path(), "docs/dev-guide/source_zh_cn").unwrap();
        assert!(dirs.contains(&"syntax".to_string()));
        assert!(dirs.contains(&"stdlib".to_string()));
        assert!(!dirs.contains(&"_hidden".to_string()));
        // Should be sorted
        assert_eq!(dirs, vec!["stdlib", "syntax"]);
    }

    #[test]
    fn test_list_md_files() {
        let (tmp, _repo) = create_test_repo();

        let files = list_md_files(tmp.path(), "docs/dev-guide/source_zh_cn/syntax").unwrap();
        assert!(files.contains(&"functions.md".to_string()));
        assert!(files.contains(&"variables.md".to_string()));
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_list_md_files_sorted() {
        let (tmp, _repo) = create_test_repo();

        let files = list_md_files(tmp.path(), "docs/dev-guide/source_zh_cn/syntax").unwrap();
        let mut sorted = files.clone();
        sorted.sort();
        assert_eq!(files, sorted);
    }

    #[test]
    fn test_open_repo() {
        let (tmp, _repo) = create_test_repo();

        let result = open_repo(tmp.path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_open_repo_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let nonexistent = tmp.path().join("nonexistent");

        let result = open_repo(&nonexistent);
        assert!(result.is_err());
    }

    #[test]
    fn test_collect_md_files_recursive_basic() {
        let (_tmp, repo) = create_test_repo();

        let head = repo.head().unwrap();
        let root_tree = head.peel_to_tree().unwrap();
        let entry = root_tree
            .get_path(Path::new("docs/dev-guide/source_zh_cn/syntax"))
            .unwrap();
        let subtree = repo.find_tree(entry.id()).unwrap();

        let mut files = Vec::new();
        collect_md_files_recursive(&repo, &subtree, "", &mut files).unwrap();

        assert!(files.contains(&"functions.md".to_string()));
        assert!(files.contains(&"variables.md".to_string()));
        assert_eq!(files.len(), 2);
    }

    #[tokio::test]
    async fn test_git_source_get_document_by_topic_without_category() {
        let (tmp, _repo) = create_test_repo();
        let source = GitDocumentSource::new(tmp.path().to_path_buf(), DocLang::Zh).unwrap();

        // Without specifying a category, it should use the topic index to find it
        let doc = source
            .get_document_by_topic("collections", None)
            .await
            .unwrap();
        assert!(doc.is_some());
        let doc = doc.unwrap();
        assert!(doc.text.contains("# Collections"));
        assert_eq!(doc.metadata.category, "stdlib");
        assert_eq!(doc.metadata.topic, "collections");
    }

    #[tokio::test]
    async fn test_git_source_get_document_by_topic_without_category_syntax() {
        let (tmp, _repo) = create_test_repo();
        let source = GitDocumentSource::new(tmp.path().to_path_buf(), DocLang::Zh).unwrap();

        let doc = source
            .get_document_by_topic("functions", None)
            .await
            .unwrap();
        assert!(doc.is_some());
        let doc = doc.unwrap();
        assert_eq!(doc.metadata.category, "syntax");
        assert_eq!(doc.metadata.topic, "functions");
        assert_eq!(doc.metadata.title, "Functions");
    }
}

// -- Remote protocol types --------------------------------------------------

#[derive(Debug, Clone, serde::Deserialize)]
struct TopicEntry {
    name: String,
    #[serde(default)]
    title: String,
}

#[derive(Debug, serde::Deserialize)]
struct RemoteTopicsResponse {
    categories: HashMap<String, Vec<TopicEntry>>,
}

#[derive(Debug, serde::Deserialize)]
struct RemoteTopicDetailResponse {
    #[serde(default)]
    content: String,
    #[serde(default)]
    file_path: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    topic: String,
    #[serde(default)]
    title: String,
}

#[derive(Debug, serde::Deserialize)]
struct RemoteHealthResponse {
    #[serde(default)]
    status: String,
}

// -- Remote Document Source --------------------------------------------------

pub struct RemoteDocumentSource {
    server_url: String,
    client: reqwest::Client,
    cache: tokio::sync::OnceCell<HashMap<String, Vec<TopicEntry>>>,
}

impl RemoteDocumentSource {
    pub fn new(server_url: &str) -> Self {
        Self {
            server_url: server_url.trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
            cache: tokio::sync::OnceCell::new(),
        }
    }

    async fn fetch_topics(&self) -> Result<HashMap<String, Vec<TopicEntry>>> {
        let url = format!("{}/topics", self.server_url);
        let resp = self.client.get(&url).send().await?;
        let data: RemoteTopicsResponse = resp.json().await.context("Invalid /topics response")?;
        Ok(data.categories)
    }

    async fn get_cached_topics(&self) -> &HashMap<String, Vec<TopicEntry>> {
        self.cache
            .get_or_init(|| async { self.fetch_topics().await.unwrap_or_default() })
            .await
    }
}

#[async_trait]
impl DocumentSource for RemoteDocumentSource {
    async fn is_available(&self) -> bool {
        let url = format!("{}/health", self.server_url);
        let resp = match self.client.get(&url).send().await {
            Ok(r) => r,
            Err(_) => return false,
        };
        resp.json::<RemoteHealthResponse>()
            .await
            .map(|h| h.status == "ok")
            .unwrap_or(false)
    }

    async fn get_categories(&self) -> Result<Vec<String>> {
        let topics = self.get_cached_topics().await;
        let mut cats: Vec<String> = topics.keys().cloned().collect();
        cats.sort();
        Ok(cats)
    }

    async fn get_topics_in_category(&self, category: &str) -> Result<Vec<String>> {
        let topics = self.get_cached_topics().await;
        let entries = topics.get(category).cloned().unwrap_or_default();
        let mut names: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();
        names.sort();
        Ok(names)
    }

    async fn get_document_by_topic(
        &self,
        topic: &str,
        category: Option<&str>,
    ) -> Result<Option<DocData>> {
        let category = match category {
            Some(c) => c.to_string(),
            None => {
                let topics = self.get_cached_topics().await;
                let mut found = None;
                for (cat, entries) in topics {
                    if entries.iter().any(|e| e.name == topic) {
                        found = Some(cat.clone());
                        break;
                    }
                }
                match found {
                    Some(c) => c,
                    None => return Ok(None),
                }
            }
        };

        let url = format!("{}/topics/{}/{}", self.server_url, category, topic);
        let resp = self.client.get(&url).send().await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let detail: RemoteTopicDetailResponse =
            resp.json().await.context("Invalid topic detail response")?;

        let file_path = if detail.file_path.is_empty() {
            format!("{category}/{topic}")
        } else {
            detail.file_path
        };

        Ok(Some(DocData {
            doc_id: file_path.clone(),
            text: detail.content,
            metadata: crate::indexer::DocMetadata {
                file_path,
                category: if detail.category.is_empty() {
                    category
                } else {
                    detail.category
                },
                topic: if detail.topic.is_empty() {
                    topic.to_string()
                } else {
                    detail.topic
                },
                title: detail.title,
                ..Default::default()
            },
        }))
    }

    async fn load_all_documents(&self) -> Result<Vec<DocData>> {
        anyhow::bail!("RemoteDocumentSource does not support load_all_documents")
    }

    async fn get_all_topic_names(&self) -> Result<Vec<String>> {
        let topics = self.get_cached_topics().await;
        let mut names: Vec<String> = topics
            .values()
            .flat_map(|entries| entries.iter().map(|e| e.name.clone()))
            .collect();
        names.sort();
        names.dedup();
        Ok(names)
    }

    async fn get_topic_titles(&self, category: &str) -> Result<HashMap<String, String>> {
        let topics = self.get_cached_topics().await;
        let entries = topics.get(category).cloned().unwrap_or_default();
        Ok(entries.into_iter().map(|e| (e.name, e.title)).collect())
    }
}
