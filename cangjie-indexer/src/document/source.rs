use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use async_trait::async_trait;
use tracing::{info, warn};

use crate::document::loader::load_document_from_content;
use crate::DocData;
use cangjie_core::config::DocLang;

#[async_trait]
pub trait DocumentSource: Send + Sync {
    async fn is_available(&self) -> bool;
    async fn load_all_documents(&self) -> Result<Vec<DocData>>;
}

// Sync gix helpers, run inside spawn_blocking.

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

/// Non-recursive sibling of `list_md_files`: returns only `.md` files at the
/// top level of `base_path` (no descent into subdirectories).
fn list_md_files_shallow(repo_dir: &Path, base_path: &str) -> Result<Vec<String>> {
    let repo = open_repo(repo_dir)?;
    let tree = repo.head_commit()?.tree()?;
    let entry = match tree.lookup_entry_by_path(base_path)? {
        Some(e) => e,
        None => return Ok(Vec::new()),
    };
    let subtree = repo.find_object(entry.oid())?.into_tree();
    let mut files = Vec::new();
    for item in subtree.iter() {
        let item = item?;
        if !item.mode().is_blob() {
            continue;
        }
        let Ok(name) = std::str::from_utf8(item.filename()) else {
            continue;
        };
        if !name.ends_with(".md") || name.starts_with('.') || name.starts_with('_') {
            continue;
        }
        files.push(name.to_string());
    }
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

pub struct GitDocumentSource {
    repo_dir: PathBuf,
    docs_base_path: String,
    category_prefix: Option<String>,
    root_category: Option<String>,
}

impl GitDocumentSource {
    fn new(
        repo_dir: PathBuf,
        docs_base_path: String,
        category_prefix: Option<String>,
        root_category: Option<String>,
    ) -> Self {
        Self {
            repo_dir,
            docs_base_path,
            category_prefix,
            root_category,
        }
    }

    pub fn for_docs(repo_dir: PathBuf, lang: DocLang) -> Result<Self> {
        Ok(Self::new(
            repo_dir,
            format!("docs/dev-guide/{}", lang.source_dir_name()),
            None,
            None,
        ))
    }

    pub fn for_runtime(repo_dir: PathBuf, lang: DocLang) -> Result<Self> {
        Ok(Self::new(
            repo_dir,
            format!("stdlib/doc/{}", lang.runtime_source_dir_name()),
            Some("stdlib".to_string()),
            None,
        ))
    }

    /// Extended stdlib (`stdx`) packages under `doc/{libs_stdx | libs_stdx_en}`.
    pub fn for_stdx(repo_dir: PathBuf, lang: DocLang) -> Result<Self> {
        Ok(Self::new(
            repo_dir,
            format!("doc/{}", lang.stdx_source_dir_name()),
            Some("stdx".to_string()),
            Some("stdx".to_string()),
        ))
    }

    /// Tooling guides under `docs/tools/{lang}` (cjpm, cjfmt, language server, etc.).
    pub fn for_tools(repo_dir: PathBuf, lang: DocLang) -> Result<Self> {
        Ok(Self::new(
            repo_dir,
            format!("docs/tools/{}", lang.source_dir_name()),
            Some("tools".to_string()),
            Some("tools".to_string()),
        ))
    }

    /// Per-version release notes under `release-notes/`. Lang-agnostic — the
    /// directory is flat and shared across UI languages.
    pub fn for_release_notes(repo_dir: PathBuf) -> Result<Self> {
        Ok(Self::new(
            repo_dir,
            "release-notes".to_string(),
            None,
            Some("release-notes".to_string()),
        ))
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
        let root_category = self.root_category.clone();

        tokio::task::spawn_blocking(move || {
            let mut documents = Vec::new();

            for category in &list_dirs(&repo_dir, &base)? {
                let path = format!("{base}/{category}");
                let display_cat = apply_prefix(&prefix, category);
                for file in &list_md_files(&repo_dir, &path)? {
                    load_md_into(&repo_dir, &path, file, &display_cat, &mut documents);
                }
            }

            if let Some(cat) = &root_category {
                for file in &list_md_files_shallow(&repo_dir, &base)? {
                    load_md_into(&repo_dir, &base, file, cat, &mut documents);
                }
            }

            info!("Loaded {} documents from git.", documents.len());
            Ok(documents)
        })
        .await
        .context("load_all_documents task panicked")?
    }
}

/// Read `dir/file` from the git tree and append the loaded document to
/// `documents`, using `category` as both the category metadata and the
/// `<category>/<file>` file_path. Errors are logged and skipped.
fn load_md_into(
    repo_dir: &Path,
    dir: &str,
    file: &str,
    category: &str,
    documents: &mut Vec<DocData>,
) {
    let full_path = format!("{dir}/{file}");
    match read_file(repo_dir, &full_path) {
        Ok(content) => {
            let topic = topic_name_from_md_path(file).unwrap_or_default();
            let relative_path = format!("{category}/{file}");
            if let Some(doc) = load_document_from_content(content, &relative_path, category, &topic)
            {
                documents.push(doc);
            }
        }
        Err(e) => warn!("Failed to load {}: {}", full_path, e),
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

    #[test]
    fn test_list_md_files_shallow_skips_subdirs_and_hidden() {
        let tmp = create_test_repo_tmp();
        // dev-guide source dir has `readme.md` at root plus subdirs (syntax,
        // stdlib, _hidden, .dotdir). Only `readme.md` should come back.
        let files = list_md_files_shallow(tmp.path(), "docs/dev-guide/source_zh_cn").unwrap();
        assert_eq!(files, vec!["readme.md".to_string()]);
    }

    #[tokio::test]
    async fn test_for_tools_loads_root_and_subcategory() {
        let tmp = create_test_repo_tmp();
        let source = GitDocumentSource::for_tools(tmp.path().to_path_buf(), DocLang::Zh).unwrap();

        let docs = source.load_all_documents().await.unwrap();
        assert_eq!(docs.len(), 2);

        let cjpm = docs
            .iter()
            .find(|d| d.metadata.topic == "cjpm_manual")
            .expect("cjpm_manual should be loaded");
        assert_eq!(cjpm.metadata.category, "tools/cmd-tools");
        assert_eq!(cjpm.metadata.file_path, "tools/cmd-tools/cjpm_manual.md");

        let overview = docs
            .iter()
            .find(|d| d.metadata.topic == "command_line_overview")
            .expect("command_line_overview should be loaded");
        assert_eq!(overview.metadata.category, "tools");
        assert_eq!(
            overview.metadata.file_path,
            "tools/command_line_overview.md"
        );
    }

    #[tokio::test]
    async fn test_for_release_notes_loads_flat_root_files() {
        let tmp = create_test_repo_tmp();
        let source = GitDocumentSource::for_release_notes(tmp.path().to_path_buf()).unwrap();

        let docs = source.load_all_documents().await.unwrap();
        assert_eq!(docs.len(), 1);

        let note = &docs[0];
        assert_eq!(note.metadata.category, "release-notes");
        assert_eq!(note.metadata.topic, "cangjie-1.1.0-release-notes");
        assert_eq!(
            note.metadata.file_path,
            "release-notes/cangjie-1.1.0-release-notes.md"
        );
    }

    #[tokio::test]
    async fn test_for_stdx_loads_nested_and_root_files() {
        let tmp = create_test_repo_tmp();
        let source = GitDocumentSource::for_stdx(tmp.path().to_path_buf(), DocLang::Zh).unwrap();

        let docs = source.load_all_documents().await.unwrap();
        assert_eq!(docs.len(), 3);

        let log_overview = docs
            .iter()
            .find(|d| d.metadata.topic == "log_package_overview")
            .expect("log_package_overview should be loaded");
        assert_eq!(log_overview.metadata.category, "stdx/log");
        assert_eq!(
            log_overview.metadata.file_path,
            "stdx/log/log_package_overview.md"
        );

        let base64 = docs
            .iter()
            .find(|d| d.metadata.topic == "base64_package_funcs")
            .expect("base64_package_funcs should be loaded recursively");
        assert_eq!(base64.metadata.category, "stdx/encoding");
        assert_eq!(
            base64.metadata.file_path,
            "stdx/encoding/base64/base64_package_api/base64_package_funcs.md"
        );

        let overview = docs
            .iter()
            .find(|d| d.metadata.topic == "libs_overview")
            .expect("libs_overview should be loaded as a root file");
        assert_eq!(overview.metadata.category, "stdx");
        assert_eq!(overview.metadata.file_path, "stdx/libs_overview.md");
    }

    #[tokio::test]
    async fn test_for_docs_still_skips_root_files() {
        // Regression: dev-guide loader must not pick up the top-level
        // `readme.md` (or `summary_*.md` in real repos) — those are TOC files,
        // not real content.
        let tmp = create_test_repo_tmp();
        let source = GitDocumentSource::for_docs(tmp.path().to_path_buf(), DocLang::Zh).unwrap();
        let docs = source.load_all_documents().await.unwrap();
        assert_eq!(docs.len(), 3);
        assert!(docs.iter().all(|d| d.metadata.topic != "readme"));
    }
}
