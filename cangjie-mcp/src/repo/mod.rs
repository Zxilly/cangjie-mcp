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
        let repo = if repo_dir.exists() {
            Repository::open(&repo_dir).ok()
        } else {
            None
        };
        Self { repo_dir, repo }
    }

    pub fn is_cloned(&self) -> bool {
        self.repo_dir.exists() && self.repo_dir.join(".git").exists()
    }

    pub fn repo(&self) -> Option<&Repository> {
        self.repo.as_ref()
    }

    pub fn clone_repo(&mut self) -> Result<()> {
        info!("Cloning repository from {}...", DOCS_REPO_URL);
        if let Some(parent) = self.repo_dir.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let repo = Repository::clone(DOCS_REPO_URL, &self.repo_dir)
            .context("Failed to clone documentation repository")?;
        self.repo = Some(repo);
        info!("Repository cloned successfully.");
        Ok(())
    }

    pub fn ensure_cloned(&mut self, fetch: bool) -> Result<()> {
        if self.is_cloned() {
            if self.repo.is_none() {
                self.repo = Some(
                    Repository::open(&self.repo_dir)
                        .context("Failed to open existing repository")?,
                );
            }
            if fetch {
                self.fetch_all()?;
            }
            Ok(())
        } else {
            self.clone_repo()
        }
    }

    fn fetch_all(&self) -> Result<()> {
        let repo = self.repo.as_ref().context("Repository not opened")?;
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

    fn sync_branch(&self) -> Result<()> {
        let repo = self.repo.as_ref().context("Repository not opened")?;
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

    pub fn checkout(&mut self, version: &str) -> Result<()> {
        self.ensure_cloned(true)?;
        let repo = self.repo.as_ref().context("Repository not opened")?;

        if version == "latest" {
            for branch in &["main", "master"] {
                let remote_ref = format!("refs/remotes/origin/{branch}");
                if let Ok(oid) = repo.refname_to_id(&remote_ref) {
                    let commit = repo.find_commit(oid)?;
                    repo.reset(commit.as_object(), git2::ResetType::Hard, None)?;
                    // Try to set HEAD to local branch
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
                    let _ = self.sync_branch();
                    info!("Checked out {} branch.", branch);
                    return Ok(());
                }
            }
        }

        // Try as tag first
        let tag_ref = format!("refs/tags/{version}");
        if let Ok(oid) = repo.refname_to_id(&tag_ref) {
            let obj = repo.find_object(oid, None)?;
            // Peel to commit (tags may be annotated)
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
            // Check if HEAD already points to this branch (e.g. default branch after clone)
            let head_is_target = repo
                .head()
                .ok()
                .and_then(|h| h.name().map(|n| n == local_ref))
                .unwrap_or(false);
            if !head_is_target {
                // Create or update local branch only when it's not the current HEAD
                repo.branch(version, &commit, true)?;
                repo.set_head(&local_ref)?;
            }
            repo.reset(commit.as_object(), git2::ResetType::Hard, None)?;
            let _ = self.sync_branch();
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

    pub fn resolve_version(&mut self, version: &str) -> Result<String> {
        self.checkout(version)?;
        let repo = self.repo.as_ref().context("Repository not opened")?;
        let head = repo.head()?;
        let commit = head.peel_to_commit()?;
        let short_hash = &commit.id().to_string()[..7];

        // Check if HEAD is detached
        if repo.head_detached().unwrap_or(true) {
            // Check if it matches any tag
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
                                return false; // stop iteration
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

    /// Read a file from the git tree at HEAD without checking out to disk.
    pub fn read_file_from_tree(&self, path: &str) -> Result<String> {
        let repo = self.repo.as_ref().context("Repository not opened")?;
        let head = repo.head()?;
        let tree = head.peel_to_tree()?;
        let entry = tree
            .get_path(Path::new(path))
            .with_context(|| format!("Path not found in tree: {path}"))?;
        let blob = repo.find_blob(entry.id())?;
        let content =
            std::str::from_utf8(blob.content()).context("File content is not valid UTF-8")?;
        Ok(content.to_string())
    }

    /// List subdirectories under a given path in the HEAD tree.
    pub fn list_tree_dirs(&self, path: &str) -> Result<Vec<String>> {
        let repo = self.repo.as_ref().context("Repository not opened")?;
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

    /// List all .md files under a path recursively. Returns relative paths from that base.
    pub fn list_md_files(&self, base_path: &str) -> Result<Vec<String>> {
        let repo = self.repo.as_ref().context("Repository not opened")?;
        let head = repo.head()?;
        let root_tree = head.peel_to_tree()?;
        let entry = root_tree.get_path(Path::new(base_path))?;
        let subtree = repo.find_tree(entry.id())?;
        let mut files = Vec::new();
        self.collect_md_files(repo, &subtree, "", &mut files)?;
        files.sort();
        Ok(files)
    }

    fn collect_md_files(
        &self,
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
                    self.collect_md_files(repo, &subtree, &path, files)?;
                }
                _ => {}
            }
        }
        Ok(())
    }
}
