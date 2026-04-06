use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use async_trait::async_trait;
use tracing::{info, warn};

use crate::document::loader::load_document_from_content;
use crate::DocData;
use cangjie_core::config::DocLang;

// -- Document Source trait ---------------------------------------------------

#[async_trait]
pub trait DocumentSource: Send + Sync {
    async fn is_available(&self) -> bool;
    async fn load_all_documents(&self) -> Result<Vec<DocData>>;
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

// -- Git Document Source -----------------------------------------------------

pub struct GitDocumentSource {
    repo_dir: PathBuf,
    docs_base_path: String,
    category_prefix: Option<String>,
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
}

#[async_trait]
impl DocumentSource for GitDocumentSource {
    async fn is_available(&self) -> bool {
        self.repo_dir.exists() && self.repo_dir.join(".git").exists()
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::create_test_repo;
    use tempfile::TempDir;

    fn create_test_repo_tmp() -> TempDir {
        create_test_repo().0
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
}
