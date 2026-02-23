use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use git2::{FetchOptions, Repository};
use tracing::{info, warn};

const DOCS_REPO_URL: &str = "https://gitcode.com/Cangjie/cangjie_docs.git";

pub struct GitManager {
    repo_dir: PathBuf,
    repo: Option<Repository>,
}

impl GitManager {
    pub fn new(repo_dir: PathBuf) -> Self {
        Self {
            repo_dir,
            repo: None,
        }
    }

    pub fn is_cloned(&self) -> bool {
        self.repo_dir.exists() && self.repo_dir.join(".git").exists()
    }

    pub fn repo(&self) -> Option<&Repository> {
        self.repo.as_ref()
    }

    fn open_or_clone(repo_dir: &Path, repo: Option<Repository>, fetch: bool) -> Result<Repository> {
        if repo_dir.exists() && repo_dir.join(".git").exists() {
            let repo = match repo {
                Some(r) => r,
                None => Repository::open(repo_dir).context("Failed to open existing repository")?,
            };
            if fetch {
                fetch_all(&repo)?;
            }
            Ok(repo)
        } else {
            info!("Cloning repository from {}...", DOCS_REPO_URL);
            if let Some(parent) = repo_dir.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let repo = Repository::clone(DOCS_REPO_URL, repo_dir)
                .context("Failed to clone documentation repository")?;
            info!("Repository cloned successfully.");
            Ok(repo)
        }
    }

    fn resolve_after_checkout(repo: &Repository) -> Result<String> {
        let head = repo.head()?;
        let commit = head.peel_to_commit()?;
        let short_hash = &commit.id().to_string()[..7];

        if repo.head_detached().unwrap_or(true) {
            let mut tag_name = None;
            repo.tag_foreach(|oid, name| {
                if let Ok(tag_obj) = repo.find_object(oid, None) {
                    if let Ok(peeled) = tag_obj.peel_to_commit() {
                        if peeled.id() == commit.id() {
                            if let Ok(name_str) = std::str::from_utf8(name) {
                                tag_name = Some(
                                    name_str
                                        .strip_prefix("refs/tags/")
                                        .unwrap_or(name_str)
                                        .to_string(),
                                );
                                return false;
                            }
                        }
                    }
                }
                true
            })?;
            if let Some(tag) = tag_name {
                return Ok(tag);
            }
            Ok(short_hash.to_string())
        } else {
            let branch = head.shorthand().unwrap_or("unknown").to_string();
            Ok(format!("{branch}({short_hash})"))
        }
    }

    pub async fn ensure_cloned(&mut self, fetch: bool) -> Result<()> {
        let repo_dir = self.repo_dir.clone();
        let repo = self.repo.take();

        let repo = tokio::task::spawn_blocking(move || Self::open_or_clone(&repo_dir, repo, fetch))
            .await
            .context("ensure_cloned task panicked")??;

        self.repo = Some(repo);
        Ok(())
    }

    pub async fn checkout(&mut self, version: &str) -> Result<()> {
        let repo_dir = self.repo_dir.clone();
        let repo = self.repo.take();
        let version = version.to_string();

        let repo = tokio::task::spawn_blocking(move || -> Result<Repository> {
            let repo = Self::open_or_clone(&repo_dir, repo, true)?;
            checkout(&repo, &version)?;
            Ok(repo)
        })
        .await
        .context("checkout task panicked")??;

        self.repo = Some(repo);
        Ok(())
    }

    pub async fn resolve_version(&mut self, version: &str) -> Result<String> {
        let repo_dir = self.repo_dir.clone();
        let repo = self.repo.take();
        let version = version.to_string();

        let (repo, resolved) =
            tokio::task::spawn_blocking(move || -> Result<(Repository, String)> {
                let repo = Self::open_or_clone(&repo_dir, repo, true)?;
                checkout(&repo, &version)?;
                let resolved = Self::resolve_after_checkout(&repo)?;
                Ok((repo, resolved))
            })
            .await
            .context("resolve_version task panicked")??;

        self.repo = Some(repo);
        Ok(resolved)
    }

    pub async fn read_file_from_tree(&self, path: &str) -> Result<String> {
        let repo_dir = self.repo_dir.clone();
        let path = path.to_string();
        tokio::task::spawn_blocking(move || read_file(&repo_dir, &path))
            .await
            .context("read_file_from_tree task panicked")?
    }

    pub async fn list_tree_dirs(&self, path: &str) -> Result<Vec<String>> {
        let repo_dir = self.repo_dir.clone();
        let path = path.to_string();
        tokio::task::spawn_blocking(move || list_tree_dirs(&repo_dir, &path))
            .await
            .context("list_tree_dirs task panicked")?
    }

    pub async fn list_md_files(&self, base_path: &str) -> Result<Vec<String>> {
        let repo_dir = self.repo_dir.clone();
        let base_path = base_path.to_string();
        tokio::task::spawn_blocking(move || list_md_files(&repo_dir, &base_path))
            .await
            .context("list_md_files task panicked")?
    }
}

fn fetch_all(repo: &Repository) -> Result<()> {
    info!("Fetching latest tags and commits...");
    let mut remote = repo.find_remote("origin")?;
    let mut fo = FetchOptions::new();
    fo.download_tags(git2::AutotagOption::All);
    fo.prune(git2::FetchPrune::On);
    match remote.fetch(&["refs/heads/*:refs/remotes/origin/*"], Some(&mut fo), None) {
        Ok(()) => {
            info!("Fetch complete.");
        }
        Err(e) => {
            warn!("Failed to fetch from remote: {}", e);
        }
    }
    Ok(())
}

fn sync_branch(repo: &Repository) -> Result<()> {
    if repo.head_detached().unwrap_or(true) {
        return Ok(());
    }
    let head = repo.head()?;
    let branch_name = head
        .shorthand()
        .context("Failed to get branch name")?
        .to_string();
    let remote_ref = format!("refs/remotes/origin/{branch_name}");
    if let Ok(remote_oid) = repo.refname_to_id(&remote_ref) {
        let remote_commit = repo.find_commit(remote_oid)?;
        repo.reset(remote_commit.as_object(), git2::ResetType::Hard, None)?;
    }
    Ok(())
}

fn checkout(repo: &Repository, version: &str) -> Result<()> {
    if version == "latest" {
        for branch in &["main", "master"] {
            let remote_ref = format!("refs/remotes/origin/{branch}");
            if let Ok(oid) = repo.refname_to_id(&remote_ref) {
                let commit = repo.find_commit(oid)?;
                repo.reset(commit.as_object(), git2::ResetType::Hard, None)?;
                let local_ref = format!("refs/heads/{branch}");
                let head_is_target = repo
                    .head()
                    .ok()
                    .and_then(|h| h.name().map(|n| n == local_ref))
                    .unwrap_or(false);
                if !head_is_target {
                    if repo.find_reference(&local_ref).is_err() {
                        repo.branch(branch, &commit, true)?;
                    }
                    repo.set_head(&local_ref)?;
                }
                let _ = sync_branch(repo);
                info!("Checked out {} branch.", branch);
                return Ok(());
            }
        }
    }

    // Try as tag first
    let tag_ref = format!("refs/tags/{version}");
    if let Ok(oid) = repo.refname_to_id(&tag_ref) {
        let obj = repo.find_object(oid, None)?;
        let commit = obj.peel_to_commit()?;
        repo.reset(commit.as_object(), git2::ResetType::Hard, None)?;
        repo.set_head_detached(commit.id())?;
        info!("Checked out tag {}.", version);
        return Ok(());
    }

    // Try as remote branch
    let remote_ref = format!("refs/remotes/origin/{version}");
    if let Ok(oid) = repo.refname_to_id(&remote_ref) {
        let commit = repo.find_commit(oid)?;
        let local_ref = format!("refs/heads/{version}");
        let head_is_target = repo
            .head()
            .ok()
            .and_then(|h| h.name().map(|n| n == local_ref))
            .unwrap_or(false);
        if !head_is_target {
            repo.branch(version, &commit, true)?;
            repo.set_head(&local_ref)?;
        }
        repo.reset(commit.as_object(), git2::ResetType::Hard, None)?;
        let _ = sync_branch(repo);
        info!("Checked out branch {}.", version);
        return Ok(());
    }

    // Try as commit hash
    if let Ok(oid) = git2::Oid::from_str(version) {
        if let Ok(commit) = repo.find_commit(oid) {
            repo.reset(commit.as_object(), git2::ResetType::Hard, None)?;
            repo.set_head_detached(commit.id())?;
            info!("Checked out commit {}.", version);
            return Ok(());
        }
    }

    bail!("Failed to checkout version '{version}': not found as tag, branch, or commit");
}

fn read_file(repo_dir: &Path, path: &str) -> Result<String> {
    let repo = Repository::open(repo_dir).context("Failed to open repository")?;
    let head = repo.head()?;
    let tree = head.peel_to_tree()?;
    let entry = tree
        .get_path(Path::new(path))
        .with_context(|| format!("Path not found in tree: {path}"))?;
    let blob = repo.find_blob(entry.id())?;
    let content = std::str::from_utf8(blob.content()).context("File content is not valid UTF-8")?;
    Ok(content.to_string())
}

fn list_tree_dirs(repo_dir: &Path, path: &str) -> Result<Vec<String>> {
    let repo = Repository::open(repo_dir).context("Failed to open repository")?;
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
    let repo = Repository::open(repo_dir).context("Failed to open repository")?;
    let head = repo.head()?;
    let root_tree = head.peel_to_tree()?;
    let entry = root_tree.get_path(Path::new(base_path))?;
    let subtree = repo.find_tree(entry.id())?;
    let mut files = Vec::new();
    collect_md_files_recursive(&repo, &subtree, "", &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_md_files_recursive(
    repo: &Repository,
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
            Some(git2::ObjectType::Blob) => {
                if name.ends_with(".md") {
                    files.push(path);
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{Repository, Signature};
    use tempfile::TempDir;

    /// Create a test git repository with a directory structure containing markdown files.
    ///
    /// Structure:
    ///   docs/dev-guide/source_zh_cn/
    ///     syntax/
    ///       functions.md
    ///       variables.md
    ///     stdlib/
    ///       collections.md
    ///     _hidden/
    ///       secret.md
    ///     .dotdir/
    ///       hidden.md
    ///     readme.md
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

        // hidden dirs (should be ignored by list_tree_dirs)
        let hidden = base.join("_hidden");
        std::fs::create_dir_all(&hidden).unwrap();
        std::fs::write(hidden.join("secret.md"), "# Secret").unwrap();

        let dotdir = base.join(".dotdir");
        std::fs::create_dir_all(&dotdir).unwrap();
        std::fs::write(dotdir.join("hidden.md"), "# Hidden").unwrap();

        // A top-level md file directly under source_zh_cn
        std::fs::write(base.join("readme.md"), "# Readme\n\nTop-level readme.").unwrap();

        // Stage and commit everything
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
    fn test_new_and_is_cloned() {
        // Before creating a repo, is_cloned should be false for a nonexistent path
        let tmp = TempDir::new().unwrap();
        let nonexistent = tmp.path().join("nonexistent");
        let mgr = GitManager::new(nonexistent);
        assert!(!mgr.is_cloned());

        // After creating a repo, is_cloned should be true
        let (tmp2, _repo) = create_test_repo();
        let mgr2 = GitManager::new(tmp2.path().to_path_buf());
        assert!(mgr2.is_cloned());
    }

    #[test]
    fn test_repo_returns_none_initially() {
        let tmp = TempDir::new().unwrap();
        let mgr = GitManager::new(tmp.path().to_path_buf());
        assert!(mgr.repo().is_none());
    }

    #[tokio::test]
    async fn test_read_file_from_tree() {
        let (tmp, _repo) = create_test_repo();
        let mgr = GitManager::new(tmp.path().to_path_buf());

        let content = mgr
            .read_file_from_tree("docs/dev-guide/source_zh_cn/syntax/functions.md")
            .await
            .unwrap();
        assert!(content.contains("# Functions"));
        assert!(content.contains("Content about functions."));
    }

    #[tokio::test]
    async fn test_read_file_not_found() {
        let (tmp, _repo) = create_test_repo();
        let mgr = GitManager::new(tmp.path().to_path_buf());

        let result = mgr
            .read_file_from_tree("docs/dev-guide/source_zh_cn/nonexistent.md")
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_tree_dirs() {
        let (tmp, _repo) = create_test_repo();
        let mgr = GitManager::new(tmp.path().to_path_buf());

        let dirs = mgr
            .list_tree_dirs("docs/dev-guide/source_zh_cn")
            .await
            .unwrap();

        // Should include stdlib and syntax but NOT _hidden or .dotdir
        assert!(dirs.contains(&"stdlib".to_string()));
        assert!(dirs.contains(&"syntax".to_string()));
        assert!(!dirs.contains(&"_hidden".to_string()));
        assert!(!dirs.contains(&".dotdir".to_string()));

        // Should be sorted
        assert_eq!(dirs, {
            let mut sorted = dirs.clone();
            sorted.sort();
            sorted
        });
    }

    #[tokio::test]
    async fn test_list_md_files() {
        let (tmp, _repo) = create_test_repo();
        let mgr = GitManager::new(tmp.path().to_path_buf());

        let files = mgr
            .list_md_files("docs/dev-guide/source_zh_cn/syntax")
            .await
            .unwrap();

        assert!(files.contains(&"functions.md".to_string()));
        assert!(files.contains(&"variables.md".to_string()));
        assert_eq!(files.len(), 2);
    }

    #[tokio::test]
    async fn test_list_md_files_nested() {
        let (tmp, _repo) = create_test_repo();
        let mgr = GitManager::new(tmp.path().to_path_buf());

        // List all md files from the base path (includes subdirectories)
        let files = mgr
            .list_md_files("docs/dev-guide/source_zh_cn")
            .await
            .unwrap();

        // Should include files from all subdirs (including hidden ones, because
        // collect_md_files_recursive does NOT filter hidden dirs, only list_tree_dirs does)
        assert!(files.contains(&"syntax/functions.md".to_string()));
        assert!(files.contains(&"syntax/variables.md".to_string()));
        assert!(files.contains(&"stdlib/collections.md".to_string()));
        assert!(files.contains(&"readme.md".to_string()));
        // Files in hidden dirs are still found by recursive traversal
        assert!(files.contains(&"_hidden/secret.md".to_string()));
        assert!(files.contains(&".dotdir/hidden.md".to_string()));
    }

    #[test]
    fn test_collect_md_ignores_non_md() {
        let tmp = TempDir::new().unwrap();
        let repo = Repository::init(tmp.path()).unwrap();

        let dir = tmp.path().join("content");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("doc.md"), "# Doc").unwrap();
        std::fs::write(dir.join("image.png"), "not-a-real-png").unwrap();
        std::fs::write(dir.join("script.js"), "console.log('hi')").unwrap();

        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = Signature::now("test", "test@test.com").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();

        let head = repo.head().unwrap();
        let root_tree = head.peel_to_tree().unwrap();
        let entry = root_tree.get_path(Path::new("content")).unwrap();
        let subtree = repo.find_tree(entry.id()).unwrap();

        let mut files = Vec::new();
        collect_md_files_recursive(&repo, &subtree, "", &mut files).unwrap();

        assert_eq!(files, vec!["doc.md".to_string()]);
    }

    #[test]
    fn test_open_or_clone_existing() {
        let (tmp, _repo) = create_test_repo();

        let result = GitManager::open_or_clone(tmp.path(), None, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_open_or_clone_with_repo_passed_in() {
        let (tmp, repo) = create_test_repo();

        let result = GitManager::open_or_clone(tmp.path(), Some(repo), false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_read_file_invalid_path() {
        let (tmp, _repo) = create_test_repo();

        let result = read_file(tmp.path(), "no/such/file.md");
        assert!(result.is_err());
    }

    #[test]
    fn test_checkout_commit_hash() {
        let (_tmp, repo) = create_test_repo();
        let head = repo.head().unwrap();
        let commit = head.peel_to_commit().unwrap();
        let hash = commit.id().to_string();

        let result = checkout(&repo, &hash);
        assert!(result.is_ok());

        assert!(repo.head_detached().unwrap());
    }

    #[test]
    fn test_checkout_nonexistent_version() {
        let (_tmp, repo) = create_test_repo();

        let result = checkout(&repo, "nonexistent-tag-or-branch");
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("not found as tag, branch, or commit"));
    }

    #[test]
    fn test_resolve_after_checkout_branch() {
        let (_tmp, repo) = create_test_repo();
        let resolved = GitManager::resolve_after_checkout(&repo).unwrap();
        assert!(
            resolved.contains("("),
            "Expected branch(hash) format, got: {}",
            resolved
        );

        let head = repo.head().unwrap();
        let commit = head.peel_to_commit().unwrap();
        let short_hash = &commit.id().to_string()[..7];
        assert!(resolved.contains(short_hash));
    }

    #[test]
    fn test_resolve_after_checkout_tag() {
        let (_tmp, repo) = create_test_repo();
        let head = repo.head().unwrap();
        let commit = head.peel_to_commit().unwrap();
        repo.tag_lightweight("v1.0.0", commit.as_object(), false)
            .unwrap();
        repo.set_head_detached(commit.id()).unwrap();

        let resolved = GitManager::resolve_after_checkout(&repo).unwrap();
        assert_eq!(resolved, "v1.0.0");
    }

    #[test]
    fn test_resolve_after_checkout_detached_no_tag() {
        let (_tmp, repo) = create_test_repo();

        let head = repo.head().unwrap();
        let commit = head.peel_to_commit().unwrap();
        let short_hash = &commit.id().to_string()[..7];
        repo.set_head_detached(commit.id()).unwrap();

        let resolved = GitManager::resolve_after_checkout(&repo).unwrap();
        assert_eq!(resolved, short_hash);
    }

    #[tokio::test]
    async fn test_ensure_cloned_existing_repo() {
        let (tmp, _repo) = create_test_repo();
        let mut mgr = GitManager::new(tmp.path().to_path_buf());
        assert!(mgr.repo().is_none());

        let result = mgr.ensure_cloned(false).await;
        assert!(result.is_ok());
        assert!(mgr.repo().is_some());
    }

    #[tokio::test]
    async fn test_list_tree_dirs_nonexistent_path() {
        let (tmp, _repo) = create_test_repo();
        let mgr = GitManager::new(tmp.path().to_path_buf());

        let result = mgr.list_tree_dirs("nonexistent/path").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_md_files_returns_sorted() {
        let (tmp, _repo) = create_test_repo();
        let mgr = GitManager::new(tmp.path().to_path_buf());

        let files = mgr
            .list_md_files("docs/dev-guide/source_zh_cn/syntax")
            .await
            .unwrap();

        let mut sorted = files.clone();
        sorted.sort();
        assert_eq!(files, sorted);
    }

    fn create_test_repo_with_remote() -> (TempDir, Repository) {
        let (tmp, repo) = create_test_repo();
        repo.remote("origin", "https://example.com/fake.git")
            .unwrap();
        let commit_id = repo.head().unwrap().peel_to_commit().unwrap().id();
        repo.reference(
            "refs/remotes/origin/main",
            commit_id,
            true,
            "create remote ref for main",
        )
        .unwrap();

        (tmp, repo)
    }

    #[test]
    fn test_checkout_latest_with_remote_main() {
        let (_tmp, repo) = create_test_repo_with_remote();

        let result = checkout(&repo, "latest");
        assert!(
            result.is_ok(),
            "checkout('latest') should succeed: {:?}",
            result.err()
        );

        let head = repo.head().unwrap();
        let head_name = head.name().unwrap_or("");
        assert!(
            head_name == "refs/heads/main" || head_name.contains("main"),
            "HEAD should point to main branch, got: {head_name}"
        );
    }

    #[test]
    fn test_checkout_latest_with_remote_master() {
        let (_tmp, repo) = create_test_repo();
        repo.remote("origin", "https://example.com/fake.git")
            .unwrap();
        let commit_id = repo.head().unwrap().peel_to_commit().unwrap().id();
        repo.reference(
            "refs/remotes/origin/master",
            commit_id,
            true,
            "create remote ref for master",
        )
        .unwrap();

        let result = checkout(&repo, "latest");
        assert!(
            result.is_ok(),
            "checkout('latest') should succeed for master: {:?}",
            result.err()
        );

        let head = repo.head().unwrap();
        let head_name = head.name().unwrap_or("");
        assert!(
            head_name == "refs/heads/master" || head_name.contains("master"),
            "HEAD should point to master branch, got: {head_name}"
        );
    }

    #[test]
    fn test_checkout_latest_no_remote_refs() {
        let (_tmp, repo) = create_test_repo();
        // No origin remote, no remote refs - "latest" should fall through and fail
        // (since there are no tags/branches/commits matching "latest")
        let result = checkout(&repo, "latest");
        assert!(
            result.is_err(),
            "checkout('latest') with no remote refs should fail"
        );
    }

    #[test]
    fn test_checkout_remote_branch() {
        let (_tmp, repo) = create_test_repo();
        repo.remote("origin", "https://example.com/fake.git")
            .unwrap();
        let commit_id = repo.head().unwrap().peel_to_commit().unwrap().id();
        repo.reference(
            "refs/remotes/origin/dev",
            commit_id,
            true,
            "create remote ref for dev",
        )
        .unwrap();

        let result = checkout(&repo, "dev");
        assert!(
            result.is_ok(),
            "checkout('dev') should succeed for remote branch: {:?}",
            result.err()
        );

        let head = repo.head().unwrap();
        let head_name = head.name().unwrap_or("");
        assert_eq!(
            head_name, "refs/heads/dev",
            "HEAD should point to local 'dev' branch created from remote, got: {head_name}"
        );
    }

    #[test]
    fn test_checkout_remote_branch_already_on_branch() {
        let (_tmp, repo) = create_test_repo();
        repo.remote("origin", "https://example.com/fake.git")
            .unwrap();
        let commit_id = repo.head().unwrap().peel_to_commit().unwrap().id();

        repo.branch("feature", &repo.find_commit(commit_id).unwrap(), false)
            .unwrap();
        repo.set_head("refs/heads/feature").unwrap();

        repo.reference(
            "refs/remotes/origin/feature",
            commit_id,
            true,
            "create remote ref for feature",
        )
        .unwrap();

        let result = checkout(&repo, "feature");
        assert!(
            result.is_ok(),
            "checkout('feature') should succeed when already on branch: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_checkout_tag() {
        let (_tmp, repo) = create_test_repo();
        let commit = repo.head().unwrap().peel_to_commit().unwrap();
        repo.tag_lightweight("v2.0.0", commit.as_object(), false)
            .unwrap();

        let result = checkout(&repo, "v2.0.0");
        assert!(
            result.is_ok(),
            "checkout('v2.0.0') should succeed for tag: {:?}",
            result.err()
        );

        assert!(repo.head_detached().unwrap());
    }

    #[test]
    fn test_sync_branch_with_remote_tracking() {
        let (_tmp, repo) = create_test_repo_with_remote();
        let commit = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("main", &commit, true).unwrap();
        repo.set_head("refs/heads/main").unwrap();

        let result = sync_branch(&repo);
        assert!(
            result.is_ok(),
            "sync_branch should succeed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_sync_branch_detached_head() {
        let (_tmp, repo) = create_test_repo();
        let commit = repo.head().unwrap().peel_to_commit().unwrap();
        repo.set_head_detached(commit.id()).unwrap();

        let result = sync_branch(&repo);
        assert!(result.is_ok());
    }

    #[test]
    fn test_sync_branch_no_remote_ref() {
        let (_tmp, repo) = create_test_repo();
        let result = sync_branch(&repo);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_checkout_async_with_remote_branch() {
        let (tmp, repo) = create_test_repo();
        repo.remote("origin", "https://example.com/fake.git")
            .unwrap();
        let commit_id = repo.head().unwrap().peel_to_commit().unwrap().id();
        repo.reference(
            "refs/remotes/origin/main",
            commit_id,
            true,
            "create remote ref",
        )
        .unwrap();
        drop(repo);

        let mut mgr = GitManager::new(tmp.path().to_path_buf());
        let result = mgr.checkout("latest").await;
        assert!(
            result.is_ok(),
            "async checkout('latest') should succeed: {:?}",
            result.err()
        );
        assert!(
            mgr.repo().is_some(),
            "repo should be populated after checkout"
        );
    }

    #[tokio::test]
    async fn test_resolve_version_async() {
        let (tmp, repo) = create_test_repo();
        repo.remote("origin", "https://example.com/fake.git")
            .unwrap();
        let commit_id = repo.head().unwrap().peel_to_commit().unwrap().id();
        repo.reference(
            "refs/remotes/origin/main",
            commit_id,
            true,
            "create remote ref",
        )
        .unwrap();
        drop(repo);

        let mut mgr = GitManager::new(tmp.path().to_path_buf());
        let resolved = mgr.resolve_version("latest").await;
        assert!(
            resolved.is_ok(),
            "resolve_version should succeed: {:?}",
            resolved.err()
        );
        let version_str = resolved.unwrap();
        assert!(
            !version_str.is_empty(),
            "resolved version should not be empty"
        );
        assert!(
            mgr.repo().is_some(),
            "repo should be populated after resolve_version"
        );
    }

    #[tokio::test]
    async fn test_resolve_version_tag() {
        let (tmp, repo) = create_test_repo();
        repo.remote("origin", "https://example.com/fake.git")
            .unwrap();
        {
            let commit = repo.head().unwrap().peel_to_commit().unwrap();
            repo.tag_lightweight("v3.0.0", commit.as_object(), false)
                .unwrap();
        }

        drop(repo);

        let mut mgr = GitManager::new(tmp.path().to_path_buf());
        let resolved = mgr.resolve_version("v3.0.0").await;
        assert!(
            resolved.is_ok(),
            "resolve_version('v3.0.0') should succeed: {:?}",
            resolved.err()
        );
        let version_str = resolved.unwrap();
        assert_eq!(
            version_str, "v3.0.0",
            "resolved version should be the tag name"
        );
    }
}
