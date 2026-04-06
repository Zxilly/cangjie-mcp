use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use async_trait::async_trait;
use tracing::{info, warn};

use crate::document::loader::{extract_title_from_content, load_document_from_content};
use crate::DocData;
use cangjie_core::config::DocLang;

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

// -- Sync gix helpers (run inside spawn_blocking) ----------------------------

fn open_repo(repo_dir: &Path) -> Result<gix::Repository> {
    gix::open(repo_dir).context("Failed to open git repository")
}

fn read_file(repo_dir: &Path, path: &str) -> Result<String> {
    let repo = open_repo(repo_dir)?;
    let tree = repo.head_commit()?.tree()?;
    let entry = tree
        .lookup_entry_by_path(path)?
        .with_context(|| format!("Path not found: {path}"))?;
    let object = repo.find_object(entry.oid())?;
    Ok(std::str::from_utf8(&object.data)?.to_string())
}

fn list_dirs(repo_dir: &Path, path: &str) -> Result<Vec<String>> {
    let repo = open_repo(repo_dir)?;
    let tree = repo.head_commit()?.tree()?;
    let entry = match tree.lookup_entry_by_path(path)? {
        Some(e) => e,
        None => return Ok(Vec::new()),
    };
    let subtree = repo.find_object(entry.oid())?.into_tree();
    let mut dirs = Vec::new();
    for item in subtree.iter() {
        let item = item?;
        if item.mode().is_tree() {
            if let Ok(name) = std::str::from_utf8(item.filename()) {
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
    let tree = repo.head_commit()?.tree()?;
    let entry = match tree.lookup_entry_by_path(base_path)? {
        Some(e) => e,
        None => return Ok(Vec::new()),
    };
    let subtree = repo.find_object(entry.oid())?.into_tree();
    let mut files = Vec::new();
    collect_md_files_recursive(&repo, &subtree, "", &mut files)?;
    files.sort();
    Ok(files)
}

fn topic_name_from_md_path(path: &str) -> Option<String> {
    path.rsplit('/')
        .next()
        .and_then(|name| name.strip_suffix(".md"))
        .map(ToString::to_string)
}

fn collect_md_files_recursive(
    repo: &gix::Repository,
    tree: &gix::Tree,
    prefix: &str,
    files: &mut Vec<String>,
) -> Result<()> {
    for item in tree.iter() {
        let item = item?;
        let name = std::str::from_utf8(item.filename()).unwrap_or("");
        let path = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}/{name}")
        };
        if item.mode().is_blob() && name.ends_with(".md") {
            files.push(path);
        } else if item.mode().is_tree() {
            let subtree = repo.find_object(item.oid())?.into_tree();
            collect_md_files_recursive(repo, &subtree, &path, files)?;
        }
    }
    Ok(())
}

fn apply_prefix(prefix: &Option<String>, cat: &str) -> String {
    match prefix {
        Some(p) => format!("{p}/{cat}"),
        None => cat.to_string(),
    }
}

fn build_topic_index(
    repo_dir: &Path,
    docs_base_path: &str,
) -> Result<HashMap<String, Vec<String>>> {
    let categories = list_dirs(repo_dir, docs_base_path)?;
    let mut mapping: HashMap<String, Vec<String>> = HashMap::new();
    for cat in &categories {
        let path = format!("{docs_base_path}/{cat}");
        match list_md_files(repo_dir, &path) {
            Ok(files) => {
                for f in &files {
                    if let Some(topic) = topic_name_from_md_path(f) {
                        mapping.entry(topic).or_default().push(cat.clone());
                    }
                }
            }
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
    category_prefix: Option<String>,
    topic_index: tokio::sync::OnceCell<HashMap<String, Vec<String>>>,
}

impl GitDocumentSource {
    pub fn new(
        repo_dir: PathBuf,
        docs_base_path: String,
        category_prefix: Option<String>,
    ) -> Result<Self> {
        Ok(Self {
            repo_dir,
            docs_base_path,
            category_prefix,
            topic_index: tokio::sync::OnceCell::new(),
        })
    }

    pub fn for_docs(repo_dir: PathBuf, lang: DocLang) -> Result<Self> {
        Self::new(
            repo_dir,
            format!("docs/dev-guide/{}", lang.source_dir_name()),
            None,
        )
    }

    pub fn for_runtime(repo_dir: PathBuf, lang: DocLang) -> Result<Self> {
        Self::new(
            repo_dir,
            format!("stdlib/doc/{}", lang.runtime_source_dir_name()),
            Some("stdlib".to_string()),
        )
    }

    fn prefixed_category(&self, cat: &str) -> String {
        apply_prefix(&self.category_prefix, cat)
    }

    fn strip_prefix<'a>(&self, category: &'a str) -> &'a str {
        match &self.category_prefix {
            Some(prefix) => category
                .strip_prefix(prefix)
                .and_then(|s| s.strip_prefix('/'))
                .unwrap_or(category),
            None => category,
        }
    }

    async fn get_cached_topic_index(&self) -> &HashMap<String, Vec<String>> {
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
        let cats = tokio::task::spawn_blocking(move || list_dirs(&repo_dir, &base))
            .await
            .context("get_categories task panicked")??;
        Ok(cats
            .into_iter()
            .map(|c| self.prefixed_category(&c))
            .collect())
    }

    async fn get_topics_in_category(&self, category: &str) -> Result<Vec<String>> {
        let repo_dir = self.repo_dir.clone();
        let raw_cat = self.strip_prefix(category);
        let path = format!("{}/{raw_cat}", self.docs_base_path);
        let files = tokio::task::spawn_blocking(move || list_md_files(&repo_dir, &path))
            .await
            .context("get_topics_in_category task panicked")??;

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
            Some(c) => self.strip_prefix(c).to_string(),
            None => match self.get_cached_topic_index().await.get(topic) {
                Some(cats) => match cats.first() {
                    Some(c) => c.clone(),
                    None => return Ok(None),
                },
                None => return Ok(None),
            },
        };

        let repo_dir = self.repo_dir.clone();
        let base = self.docs_base_path.clone();
        let topic = topic.to_string();
        let cat = category.clone();
        let prefix = self.category_prefix.clone();

        tokio::task::spawn_blocking(move || {
            let filename = format!("{topic}.md");
            let path = format!("{base}/{cat}");
            let files = list_md_files(&repo_dir, &path)?;
            let display_cat = apply_prefix(&prefix, &cat);
            for file in &files {
                let file_name = file.rsplit('/').next().unwrap_or("");
                if file_name == filename {
                    let full_path = format!("{path}/{file}");
                    match read_file(&repo_dir, &full_path) {
                        Ok(content) => {
                            let relative_path = format!("{display_cat}/{file}");
                            return Ok(load_document_from_content(
                                content,
                                &relative_path,
                                &display_cat,
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
        let prefix = self.category_prefix.clone();

        tokio::task::spawn_blocking(move || {
            let categories = list_dirs(&repo_dir, &base)?;
            let mut documents = Vec::new();

            for category in &categories {
                let path = format!("{base}/{category}");
                let files = list_md_files(&repo_dir, &path)?;
                let display_cat = apply_prefix(&prefix, category);

                for file in &files {
                    let full_path = format!("{path}/{file}");
                    match read_file(&repo_dir, &full_path) {
                        Ok(content) => {
                            let topic = topic_name_from_md_path(file).unwrap_or_default();
                            let relative_path = format!("{display_cat}/{file}");
                            if let Some(doc) = load_document_from_content(
                                content,
                                &relative_path,
                                &display_cat,
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
        let category = self.strip_prefix(category).to_string();

        tokio::task::spawn_blocking(move || {
            let path = format!("{base}/{category}");
            let files = list_md_files(&repo_dir, &path)?;
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
    use crate::testutil::{create_test_repo, git_init_and_commit};
    use tempfile::TempDir;

    fn create_test_repo_tmp() -> TempDir {
        create_test_repo().0
    }

    fn create_test_repo_without_docs_base() -> TempDir {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("README.md"), "# Placeholder").unwrap();
        git_init_and_commit(tmp.path());
        tmp
    }

    #[tokio::test]
    async fn test_git_source_is_available() {
        let tmp = create_test_repo_tmp();
        let source = GitDocumentSource::for_docs(tmp.path().to_path_buf(), DocLang::Zh).unwrap();
        assert!(source.is_available().await);
    }

    #[tokio::test]
    async fn test_git_source_not_available() {
        let tmp = TempDir::new().unwrap();
        let nonexistent = tmp.path().join("nonexistent");
        let source = GitDocumentSource::for_docs(nonexistent, DocLang::Zh).unwrap();
        assert!(!source.is_available().await);
    }

    #[tokio::test]
    async fn test_git_source_not_available_no_git_dir() {
        let tmp = TempDir::new().unwrap();
        let source = GitDocumentSource::for_docs(tmp.path().to_path_buf(), DocLang::Zh).unwrap();
        assert!(!source.is_available().await);
    }

    #[tokio::test]
    async fn test_git_source_get_categories() {
        let tmp = create_test_repo_tmp();
        let source = GitDocumentSource::for_docs(tmp.path().to_path_buf(), DocLang::Zh).unwrap();

        let categories = source.get_categories().await.unwrap();
        assert!(categories.contains(&"stdlib".to_string()));
        assert!(categories.contains(&"syntax".to_string()));
        assert!(!categories.contains(&"_hidden".to_string()));
        assert_eq!(categories, vec!["stdlib", "syntax"]);
    }

    #[tokio::test]
    async fn test_git_source_get_categories_without_docs_base_returns_empty() {
        let tmp = create_test_repo_without_docs_base();
        let source = GitDocumentSource::for_docs(tmp.path().to_path_buf(), DocLang::Zh).unwrap();

        let categories = source.get_categories().await.unwrap();
        assert!(categories.is_empty());
    }

    #[tokio::test]
    async fn test_git_source_get_topics_in_category() {
        let tmp = create_test_repo_tmp();
        let source = GitDocumentSource::for_docs(tmp.path().to_path_buf(), DocLang::Zh).unwrap();

        let topics = source.get_topics_in_category("syntax").await.unwrap();
        assert!(topics.contains(&"functions".to_string()));
        assert!(topics.contains(&"variables".to_string()));
        assert_eq!(topics.len(), 2);
    }

    #[tokio::test]
    async fn test_git_source_get_topics_in_category_stdlib() {
        let tmp = create_test_repo_tmp();
        let source = GitDocumentSource::for_docs(tmp.path().to_path_buf(), DocLang::Zh).unwrap();

        let topics = source.get_topics_in_category("stdlib").await.unwrap();
        assert_eq!(topics, vec!["collections"]);
    }

    #[tokio::test]
    async fn test_git_source_get_document_by_topic() {
        let tmp = create_test_repo_tmp();
        let source = GitDocumentSource::for_docs(tmp.path().to_path_buf(), DocLang::Zh).unwrap();

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
        let tmp = create_test_repo_tmp();
        let source = GitDocumentSource::for_docs(tmp.path().to_path_buf(), DocLang::Zh).unwrap();

        let doc = source
            .get_document_by_topic("nonexistent", Some("syntax"))
            .await
            .unwrap();
        assert!(doc.is_none());
    }

    #[tokio::test]
    async fn test_git_source_get_document_not_found_in_nonexistent_category() {
        let tmp = create_test_repo_tmp();
        let source = GitDocumentSource::for_docs(tmp.path().to_path_buf(), DocLang::Zh).unwrap();

        let doc = source
            .get_document_by_topic("functions", Some("nonexistent_category_xyz"))
            .await
            .unwrap();
        assert!(doc.is_none());
    }

    #[tokio::test]
    async fn test_git_source_get_document_not_found_no_category() {
        let tmp = create_test_repo_tmp();
        let source = GitDocumentSource::for_docs(tmp.path().to_path_buf(), DocLang::Zh).unwrap();

        let doc = source
            .get_document_by_topic("totally_nonexistent", None)
            .await
            .unwrap();
        assert!(doc.is_none());
    }

    #[tokio::test]
    async fn test_git_source_load_all_documents() {
        let tmp = create_test_repo_tmp();
        let source = GitDocumentSource::for_docs(tmp.path().to_path_buf(), DocLang::Zh).unwrap();

        let docs = source.load_all_documents().await.unwrap();
        assert_eq!(docs.len(), 3);

        let topics: Vec<&str> = docs.iter().map(|d| d.metadata.topic.as_str()).collect();
        assert!(topics.contains(&"functions"));
        assert!(topics.contains(&"variables"));
        assert!(topics.contains(&"collections"));
    }

    #[tokio::test]
    async fn test_git_source_get_all_topic_names() {
        let tmp = create_test_repo_tmp();
        let source = GitDocumentSource::for_docs(tmp.path().to_path_buf(), DocLang::Zh).unwrap();

        let names = source.get_all_topic_names().await.unwrap();
        assert!(names.contains(&"functions".to_string()));
        assert!(names.contains(&"variables".to_string()));
        assert!(names.contains(&"collections".to_string()));
        assert_eq!(names, {
            let mut sorted = names.clone();
            sorted.sort();
            sorted
        });
    }

    #[tokio::test]
    async fn test_git_source_get_topic_titles() {
        let tmp = create_test_repo_tmp();
        let source = GitDocumentSource::for_docs(tmp.path().to_path_buf(), DocLang::Zh).unwrap();

        let titles = source.get_topic_titles("syntax").await.unwrap();
        assert_eq!(titles.get("functions").unwrap(), "Functions");
        assert_eq!(titles.get("variables").unwrap(), "Variables");
        assert_eq!(titles.len(), 2);
    }

    #[tokio::test]
    async fn test_git_source_get_topic_titles_stdlib() {
        let tmp = create_test_repo_tmp();
        let source = GitDocumentSource::for_docs(tmp.path().to_path_buf(), DocLang::Zh).unwrap();

        let titles = source.get_topic_titles("stdlib").await.unwrap();
        assert_eq!(titles.get("collections").unwrap(), "Collections");
        assert_eq!(titles.len(), 1);
    }

    #[test]
    fn test_build_topic_index() {
        let tmp = create_test_repo_tmp();

        let index = build_topic_index(tmp.path(), "docs/dev-guide/source_zh_cn").unwrap();
        assert_eq!(index.get("functions").unwrap(), &vec!["syntax".to_string()]);
        assert_eq!(index.get("variables").unwrap(), &vec!["syntax".to_string()]);
        assert_eq!(
            index.get("collections").unwrap(),
            &vec!["stdlib".to_string()]
        );
        assert_eq!(index.len(), 3);
    }

    #[test]
    fn test_read_file() {
        let tmp = create_test_repo_tmp();

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
        let tmp = create_test_repo_tmp();

        let result = read_file(tmp.path(), "nonexistent/file.md");
        assert!(result.is_err());
    }

    #[test]
    fn test_list_dirs() {
        let tmp = create_test_repo_tmp();

        let dirs = list_dirs(tmp.path(), "docs/dev-guide/source_zh_cn").unwrap();
        assert!(dirs.contains(&"syntax".to_string()));
        assert!(dirs.contains(&"stdlib".to_string()));
        assert!(!dirs.contains(&"_hidden".to_string()));
        assert_eq!(dirs, vec!["stdlib", "syntax"]);
    }

    #[test]
    fn test_list_md_files() {
        let tmp = create_test_repo_tmp();

        let files = list_md_files(tmp.path(), "docs/dev-guide/source_zh_cn/syntax").unwrap();
        assert!(files.contains(&"functions.md".to_string()));
        assert!(files.contains(&"variables.md".to_string()));
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_collect_md_files_recursive_basic() {
        let tmp = create_test_repo_tmp();
        let repo = gix::open(tmp.path()).unwrap();

        let tree = repo.head_commit().unwrap().tree().unwrap();
        let entry = tree
            .lookup_entry_by_path("docs/dev-guide/source_zh_cn/syntax")
            .unwrap()
            .unwrap();
        let subtree = repo.find_object(entry.oid()).unwrap().into_tree();

        let mut files = Vec::new();
        collect_md_files_recursive(&repo, &subtree, "", &mut files).unwrap();

        assert!(files.contains(&"functions.md".to_string()));
        assert!(files.contains(&"variables.md".to_string()));
        assert_eq!(files.len(), 2);
    }

    #[tokio::test]
    async fn test_git_source_get_document_by_topic_without_category() {
        let tmp = create_test_repo_tmp();
        let source = GitDocumentSource::for_docs(tmp.path().to_path_buf(), DocLang::Zh).unwrap();

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
        let tmp = create_test_repo_tmp();
        let source = GitDocumentSource::for_docs(tmp.path().to_path_buf(), DocLang::Zh).unwrap();

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
