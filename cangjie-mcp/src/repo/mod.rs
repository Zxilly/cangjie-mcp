use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit, RefLog};
use gix::refs::Target;
use tracing::{info, warn};

const DOCS_REPO_URL: &str = "https://gitcode.com/Cangjie/cangjie_docs.git";

pub struct GitManager {
    repo_dir: PathBuf,
    repo: Option<gix::Repository>,
}

/// Helper to create a RefEdit that sets a ref to point at an object (detached).
fn ref_edit_to_object(name: &str, oid: gix::ObjectId, msg: &str) -> Result<RefEdit> {
    Ok(RefEdit {
        change: Change::Update {
            log: LogChange {
                mode: RefLog::AndReference,
                force_create_reflog: false,
                message: msg.into(),
            },
            expected: PreviousValue::Any,
            new: Target::Object(oid),
        },
        name: name.try_into()?,
        deref: false,
    })
}

/// Helper to create a RefEdit that sets a ref to point symbolically at another ref.
fn ref_edit_symbolic(name: &str, target_ref: &str, msg: &str) -> Result<RefEdit> {
    Ok(RefEdit {
        change: Change::Update {
            log: LogChange {
                mode: RefLog::AndReference,
                force_create_reflog: false,
                message: msg.into(),
            },
            expected: PreviousValue::Any,
            new: Target::Symbolic(target_ref.try_into()?),
        },
        name: name.try_into()?,
        deref: false,
    })
}

fn ensure_committer_for_ref_edits(repo: &mut gix::Repository) -> Result<()> {
    match repo.committer() {
        Some(Ok(_)) => return Ok(()),
        Some(Err(err)) => {
            return Err(anyhow::Error::new(err))
                .context("Invalid committer configuration for reflog writes");
        }
        None => {}
    }

    // gix requires committer identity to write reflog entries during ref edits.
    let mut repo_config = repo.config_snapshot_mut();
    repo_config
        .set_value(
            &gix::config::tree::gitoxide::Committer::NAME_FALLBACK,
            "cangjie-mcp",
        )
        .context("Failed to set in-memory committer name fallback")?;
    repo_config
        .set_value(
            &gix::config::tree::gitoxide::Committer::EMAIL_FALLBACK,
            "no-email@cangjie-mcp.local",
        )
        .context("Failed to set in-memory committer email fallback")?;
    repo_config
        .commit()
        .context("Failed to apply in-memory committer fallback")?;

    match repo.committer() {
        Some(Ok(_)) => Ok(()),
        Some(Err(err)) => Err(anyhow::Error::new(err))
            .context("Invalid committer configuration after applying fallback"),
        None => bail!("No committer is configured and fallback committer could not be applied"),
    }
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

    pub fn repo(&self) -> Option<&gix::Repository> {
        self.repo.as_ref()
    }

    fn open_or_clone(
        repo_dir: &Path,
        repo: Option<gix::Repository>,
        fetch: bool,
    ) -> Result<gix::Repository> {
        if repo_dir.exists() && repo_dir.join(".git").exists() {
            let mut repo = match repo {
                Some(r) => r,
                None => gix::open(repo_dir).context("Failed to open existing repository")?,
            };
            ensure_committer_for_ref_edits(&mut repo)?;
            if fetch {
                fetch_all(&repo)?;
            }
            Ok(repo)
        } else {
            info!("Cloning repository from {}...", DOCS_REPO_URL);
            if let Some(parent) = repo_dir.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let (mut checkout, _) = gix::prepare_clone(DOCS_REPO_URL, repo_dir)
                .context("Failed to prepare clone")?
                .fetch_then_checkout(gix::progress::Discard, &gix::interrupt::IS_INTERRUPTED)
                .context("Failed to fetch during clone")?;
            let (mut repo, _) = checkout
                .main_worktree(gix::progress::Discard, &gix::interrupt::IS_INTERRUPTED)
                .context("Failed to checkout worktree during clone")?;
            ensure_committer_for_ref_edits(&mut repo)?;
            info!("Repository cloned successfully.");
            Ok(repo)
        }
    }

    fn resolve_after_checkout(repo: &gix::Repository) -> Result<String> {
        let mut head = repo.head().context("Failed to read HEAD")?;
        let commit = head
            .peel_to_commit()
            .context("Failed to peel HEAD to commit")?;
        let short_hash = &commit.id().to_string()[..7];

        if head.is_detached() {
            // Look for a tag pointing to this commit
            let commit_id = commit.id().detach();
            let mut tag_name = None;
            if let Ok(refs) = repo.references() {
                if let Ok(tag_refs) = refs.tags() {
                    for reference in tag_refs.flatten() {
                        if let Ok(peeled) = reference.clone().into_fully_peeled_id() {
                            if peeled.detach() == commit_id {
                                let name = reference.name().shorten().to_string();
                                tag_name = Some(name);
                                break;
                            }
                        }
                    }
                }
            }
            if let Some(tag) = tag_name {
                return Ok(tag);
            }
            Ok(short_hash.to_string())
        } else {
            let branch = head
                .referent_name()
                .map(|n| n.shorten().to_string())
                .unwrap_or_else(|| "unknown".to_string());
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

        let repo = tokio::task::spawn_blocking(move || -> Result<gix::Repository> {
            let mut repo = Self::open_or_clone(&repo_dir, repo, true)?;
            checkout(&mut repo, &version)?;
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
            tokio::task::spawn_blocking(move || -> Result<(gix::Repository, String)> {
                let mut repo = Self::open_or_clone(&repo_dir, repo, true)?;
                checkout(&mut repo, &version)?;
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

fn fetch_all(repo: &gix::Repository) -> Result<()> {
    info!("Fetching latest tags and commits...");
    match do_fetch(repo) {
        Ok(()) => info!("Fetch complete."),
        Err(e) => warn!("Failed to fetch from remote: {e}"),
    }
    Ok(())
}

fn do_fetch(repo: &gix::Repository) -> Result<()> {
    let remote = repo.find_remote("origin")?;
    let tagged = remote.with_fetch_tags(gix::remote::fetch::Tags::All);
    let conn = tagged.connect(gix::remote::Direction::Fetch)?;
    let prep = conn.prepare_fetch(gix::progress::Discard, Default::default())?;
    prep.receive(gix::progress::Discard, &gix::interrupt::IS_INTERRUPTED)?;
    Ok(())
}

fn sync_branch(repo: &mut gix::Repository) -> Result<()> {
    ensure_committer_for_ref_edits(repo)?;
    let head = repo.head().context("Failed to read HEAD")?;
    if head.is_detached() {
        return Ok(());
    }
    let branch_name = head
        .referent_name()
        .map(|n| n.shorten().to_string())
        .context("Failed to get branch name")?;
    let remote_ref = format!("refs/remotes/origin/{branch_name}");
    if let Ok(mut remote_reference) = repo.find_reference(&remote_ref) {
        let remote_id = remote_reference
            .peel_to_id()
            .context("Failed to peel remote ref")?
            .detach();
        // Update local branch ref to match remote
        let local_ref = format!("refs/heads/{branch_name}");
        repo.edit_reference(ref_edit_to_object(
            &local_ref,
            remote_id,
            "sync branch to remote",
        )?)?;
    }
    Ok(())
}

fn checkout(repo: &mut gix::Repository, version: &str) -> Result<()> {
    ensure_committer_for_ref_edits(repo)?;
    if version == "latest" {
        for branch in &["main", "master"] {
            let remote_ref = format!("refs/remotes/origin/{branch}");
            if let Ok(mut reference) = repo.find_reference(&remote_ref) {
                let oid = reference
                    .peel_to_id()
                    .context("Failed to peel remote ref")?
                    .detach();

                let local_ref = format!("refs/heads/{branch}");
                let head_is_target = repo
                    .head()
                    .ok()
                    .and_then(|h| h.referent_name().map(|n| n.as_bstr().to_string()))
                    .map(|n| n == local_ref)
                    .unwrap_or(false);

                if !head_is_target {
                    // Create/update local branch ref pointing at oid
                    repo.edit_reference(ref_edit_to_object(
                        &local_ref,
                        oid,
                        "create local branch from remote",
                    )?)?;
                    // Set HEAD to point symbolically to the local branch
                    repo.edit_reference(ref_edit_symbolic("HEAD", &local_ref, "checkout branch")?)?;
                }
                let _ = sync_branch(repo);
                info!("Checked out {} branch.", branch);
                return Ok(());
            }
        }
    }

    // Try as tag first
    let tag_ref = format!("refs/tags/{version}");
    if let Ok(mut reference) = repo.find_reference(&tag_ref) {
        let oid = reference
            .peel_to_id()
            .context("Failed to peel tag ref")?
            .detach();
        // Set HEAD detached to the tag's commit
        repo.edit_reference(ref_edit_to_object("HEAD", oid, "checkout tag")?)?;
        info!("Checked out tag {}.", version);
        return Ok(());
    }

    // Try as remote branch
    let remote_ref = format!("refs/remotes/origin/{version}");
    if let Ok(mut reference) = repo.find_reference(&remote_ref) {
        let oid = reference
            .peel_to_id()
            .context("Failed to peel remote ref")?
            .detach();
        let local_ref = format!("refs/heads/{version}");
        let head_is_target = repo
            .head()
            .ok()
            .and_then(|h| h.referent_name().map(|n| n.as_bstr().to_string()))
            .map(|n| n == local_ref)
            .unwrap_or(false);
        if !head_is_target {
            repo.edit_reference(ref_edit_to_object(
                &local_ref,
                oid,
                "create local branch from remote",
            )?)?;
            repo.edit_reference(ref_edit_symbolic("HEAD", &local_ref, "checkout branch")?)?;
        }
        // Update local branch to match remote
        repo.edit_reference(ref_edit_to_object(
            &local_ref,
            oid,
            "sync local branch to remote",
        )?)?;
        let _ = sync_branch(repo);
        info!("Checked out branch {}.", version);
        return Ok(());
    }

    // Try as commit hash
    if let Ok(oid) = gix::ObjectId::from_hex(version.as_bytes()) {
        if repo.find_commit(oid).is_ok() {
            repo.edit_reference(ref_edit_to_object("HEAD", oid, "checkout commit")?)?;
            info!("Checked out commit {}.", version);
            return Ok(());
        }
    }

    bail!("Failed to checkout version '{version}': not found as tag, branch, or commit");
}

fn read_file(repo_dir: &Path, path: &str) -> Result<String> {
    let repo = gix::open(repo_dir).context("Failed to open repository")?;
    let tree = repo.head_commit()?.tree()?;
    let entry = tree
        .lookup_entry_by_path(path)?
        .with_context(|| format!("Path not found in tree: {path}"))?;
    let object = repo.find_object(entry.oid())?;
    let content = std::str::from_utf8(&object.data).context("File content is not valid UTF-8")?;
    Ok(content.to_string())
}

fn list_tree_dirs(repo_dir: &Path, path: &str) -> Result<Vec<String>> {
    let repo = gix::open(repo_dir).context("Failed to open repository")?;
    let tree = repo.head_commit()?.tree()?;
    let entry = tree
        .lookup_entry_by_path(path)?
        .with_context(|| format!("Path not found in tree: {path}"))?;
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
    let repo = gix::open(repo_dir).context("Failed to open repository")?;
    let tree = repo.head_commit()?.tree()?;
    let entry = tree
        .lookup_entry_by_path(base_path)?
        .with_context(|| format!("Path not found in tree: {base_path}"))?;
    let subtree = repo.find_object(entry.oid())?.into_tree();
    let mut files = Vec::new();
    collect_md_files_recursive(&repo, &subtree, "", &mut files)?;
    files.sort();
    Ok(files)
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
        if item.mode().is_blob() {
            if name.ends_with(".md") {
                files.push(path);
            }
        } else if item.mode().is_tree() {
            let subtree = repo.find_object(item.oid())?.into_tree();
            collect_md_files_recursive(repo, &subtree, &path, files)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    /// Create a test git repository with a directory structure containing markdown files.
    fn create_test_repo() -> (TempDir, gix::Repository) {
        let tmp = TempDir::new().unwrap();

        let base = tmp
            .path()
            .join("docs")
            .join("dev-guide")
            .join("source_zh_cn");

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

        let stdlib_dir = base.join("stdlib");
        std::fs::create_dir_all(&stdlib_dir).unwrap();
        std::fs::write(
            stdlib_dir.join("collections.md"),
            "# Collections\n\nContent about collections.",
        )
        .unwrap();

        let hidden = base.join("_hidden");
        std::fs::create_dir_all(&hidden).unwrap();
        std::fs::write(hidden.join("secret.md"), "# Secret").unwrap();

        let dotdir = base.join(".dotdir");
        std::fs::create_dir_all(&dotdir).unwrap();
        std::fs::write(dotdir.join("hidden.md"), "# Hidden").unwrap();

        std::fs::write(base.join("readme.md"), "# Readme\n\nTop-level readme.").unwrap();

        Command::new("git")
            .args(["init"])
            .current_dir(tmp.path())
            .status()
            .unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(tmp.path())
            .status()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "initial commit"])
            .env("GIT_AUTHOR_NAME", "test")
            .env("GIT_AUTHOR_EMAIL", "test@test.com")
            .env("GIT_COMMITTER_NAME", "test")
            .env("GIT_COMMITTER_EMAIL", "test@test.com")
            .current_dir(tmp.path())
            .status()
            .unwrap();

        let repo = gix::open(tmp.path()).unwrap();
        (tmp, repo)
    }

    #[test]
    fn test_new_and_is_cloned() {
        let tmp = TempDir::new().unwrap();
        let nonexistent = tmp.path().join("nonexistent");
        let mgr = GitManager::new(nonexistent);
        assert!(!mgr.is_cloned());

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

        assert!(dirs.contains(&"stdlib".to_string()));
        assert!(dirs.contains(&"syntax".to_string()));
        assert!(!dirs.contains(&"_hidden".to_string()));
        assert!(!dirs.contains(&".dotdir".to_string()));

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

        let files = mgr
            .list_md_files("docs/dev-guide/source_zh_cn")
            .await
            .unwrap();

        assert!(files.contains(&"syntax/functions.md".to_string()));
        assert!(files.contains(&"syntax/variables.md".to_string()));
        assert!(files.contains(&"stdlib/collections.md".to_string()));
        assert!(files.contains(&"readme.md".to_string()));
        assert!(files.contains(&"_hidden/secret.md".to_string()));
        assert!(files.contains(&".dotdir/hidden.md".to_string()));
    }

    #[test]
    fn test_collect_md_ignores_non_md() {
        let tmp = TempDir::new().unwrap();

        let dir = tmp.path().join("content");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("doc.md"), "# Doc").unwrap();
        std::fs::write(dir.join("image.png"), "not-a-real-png").unwrap();
        std::fs::write(dir.join("script.js"), "console.log('hi')").unwrap();

        Command::new("git")
            .args(["init"])
            .current_dir(tmp.path())
            .status()
            .unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(tmp.path())
            .status()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .env("GIT_AUTHOR_NAME", "test")
            .env("GIT_AUTHOR_EMAIL", "test@test.com")
            .env("GIT_COMMITTER_NAME", "test")
            .env("GIT_COMMITTER_EMAIL", "test@test.com")
            .current_dir(tmp.path())
            .status()
            .unwrap();

        let repo = gix::open(tmp.path()).unwrap();
        let tree = repo.head_commit().unwrap().tree().unwrap();
        let entry = tree.lookup_entry_by_path("content").unwrap().unwrap();
        let subtree = repo.find_object(entry.oid()).unwrap().into_tree();

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
        let (tmp, _repo) = create_test_repo();
        let mut repo = gix::open(tmp.path()).unwrap();
        let hash = {
            let commit = repo.head_commit().unwrap();
            commit.id().to_string()
        };

        let result = checkout(&mut repo, &hash);
        assert!(result.is_ok());

        assert!(repo.head().unwrap().is_detached());
    }

    #[test]
    fn test_checkout_nonexistent_version() {
        let (tmp, _repo) = create_test_repo();
        let mut repo = gix::open(tmp.path()).unwrap();

        let result = checkout(&mut repo, "nonexistent-tag-or-branch");
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("not found as tag, branch, or commit"));
    }

    #[test]
    fn test_resolve_after_checkout_branch() {
        let (tmp, _repo) = create_test_repo();
        let repo = gix::open(tmp.path()).unwrap();
        let resolved = GitManager::resolve_after_checkout(&repo).unwrap();
        assert!(
            resolved.contains("("),
            "Expected branch(hash) format, got: {}",
            resolved
        );

        let commit = repo.head_commit().unwrap();
        let short_hash = &commit.id().to_string()[..7];
        assert!(resolved.contains(short_hash));
    }

    #[test]
    fn test_resolve_after_checkout_tag() {
        let (tmp, _repo) = create_test_repo();
        Command::new("git")
            .args(["tag", "v1.0.0"])
            .current_dir(tmp.path())
            .status()
            .unwrap();
        Command::new("git")
            .args(["checkout", "--detach", "HEAD"])
            .current_dir(tmp.path())
            .status()
            .unwrap();

        let repo = gix::open(tmp.path()).unwrap();
        let resolved = GitManager::resolve_after_checkout(&repo).unwrap();
        assert_eq!(resolved, "v1.0.0");
    }

    #[test]
    fn test_resolve_after_checkout_detached_no_tag() {
        let (tmp, _repo) = create_test_repo();
        Command::new("git")
            .args(["checkout", "--detach", "HEAD"])
            .current_dir(tmp.path())
            .status()
            .unwrap();

        let repo = gix::open(tmp.path()).unwrap();
        let commit = repo.head_commit().unwrap();
        let short_hash = &commit.id().to_string()[..7];

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

    fn create_test_repo_with_remote() -> (TempDir, gix::Repository) {
        let (tmp, _) = create_test_repo();
        Command::new("git")
            .args(["remote", "add", "origin", "https://example.com/fake.git"])
            .current_dir(tmp.path())
            .status()
            .unwrap();
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        let commit_hash = String::from_utf8(output.stdout).unwrap().trim().to_string();
        Command::new("git")
            .args(["update-ref", "refs/remotes/origin/main", &commit_hash])
            .current_dir(tmp.path())
            .status()
            .unwrap();

        let repo = gix::open(tmp.path()).unwrap();
        (tmp, repo)
    }

    #[test]
    fn test_checkout_latest_with_remote_main() {
        let (tmp, _repo) = create_test_repo_with_remote();
        let mut repo = gix::open(tmp.path()).unwrap();

        let result = checkout(&mut repo, "latest");
        assert!(
            result.is_ok(),
            "checkout('latest') should succeed: {:?}",
            result.err()
        );

        let head = repo.head().unwrap();
        let head_name = head
            .referent_name()
            .map(|n| n.as_bstr().to_string())
            .unwrap_or_default();
        assert!(
            head_name == "refs/heads/main" || head_name.contains("main"),
            "HEAD should point to main branch, got: {head_name}"
        );
    }

    #[test]
    fn test_checkout_latest_with_remote_master() {
        let (tmp, _repo) = create_test_repo();
        Command::new("git")
            .args(["remote", "add", "origin", "https://example.com/fake.git"])
            .current_dir(tmp.path())
            .status()
            .unwrap();
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        let commit_hash = String::from_utf8(output.stdout).unwrap().trim().to_string();
        Command::new("git")
            .args(["update-ref", "refs/remotes/origin/master", &commit_hash])
            .current_dir(tmp.path())
            .status()
            .unwrap();

        let mut repo = gix::open(tmp.path()).unwrap();
        let result = checkout(&mut repo, "latest");
        assert!(
            result.is_ok(),
            "checkout('latest') should succeed for master: {:?}",
            result.err()
        );

        let head = repo.head().unwrap();
        let head_name = head
            .referent_name()
            .map(|n| n.as_bstr().to_string())
            .unwrap_or_default();
        assert!(
            head_name == "refs/heads/master" || head_name.contains("master"),
            "HEAD should point to master branch, got: {head_name}"
        );
    }

    #[test]
    fn test_checkout_latest_no_remote_refs() {
        let (tmp, _repo) = create_test_repo();
        let mut repo = gix::open(tmp.path()).unwrap();

        let result = checkout(&mut repo, "latest");
        assert!(
            result.is_err(),
            "checkout('latest') with no remote refs should fail"
        );
    }

    #[test]
    fn test_checkout_remote_branch() {
        let (tmp, _repo) = create_test_repo();
        Command::new("git")
            .args(["remote", "add", "origin", "https://example.com/fake.git"])
            .current_dir(tmp.path())
            .status()
            .unwrap();
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        let commit_hash = String::from_utf8(output.stdout).unwrap().trim().to_string();
        Command::new("git")
            .args(["update-ref", "refs/remotes/origin/dev", &commit_hash])
            .current_dir(tmp.path())
            .status()
            .unwrap();

        let mut repo = gix::open(tmp.path()).unwrap();
        let result = checkout(&mut repo, "dev");
        assert!(
            result.is_ok(),
            "checkout('dev') should succeed for remote branch: {:?}",
            result.err()
        );

        let head = repo.head().unwrap();
        let head_name = head
            .referent_name()
            .map(|n| n.as_bstr().to_string())
            .unwrap_or_default();
        assert_eq!(
            head_name, "refs/heads/dev",
            "HEAD should point to local 'dev' branch created from remote, got: {head_name}"
        );
    }

    #[test]
    fn test_checkout_remote_branch_already_on_branch() {
        let (tmp, _repo) = create_test_repo();
        Command::new("git")
            .args(["remote", "add", "origin", "https://example.com/fake.git"])
            .current_dir(tmp.path())
            .status()
            .unwrap();
        Command::new("git")
            .args(["branch", "feature"])
            .current_dir(tmp.path())
            .status()
            .unwrap();
        Command::new("git")
            .args(["checkout", "feature"])
            .current_dir(tmp.path())
            .status()
            .unwrap();
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        let commit_hash = String::from_utf8(output.stdout).unwrap().trim().to_string();
        Command::new("git")
            .args(["update-ref", "refs/remotes/origin/feature", &commit_hash])
            .current_dir(tmp.path())
            .status()
            .unwrap();

        let mut repo = gix::open(tmp.path()).unwrap();
        let result = checkout(&mut repo, "feature");
        assert!(
            result.is_ok(),
            "checkout('feature') should succeed when already on branch: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_checkout_tag() {
        let (tmp, _repo) = create_test_repo();
        Command::new("git")
            .args(["tag", "v2.0.0"])
            .current_dir(tmp.path())
            .status()
            .unwrap();

        let mut repo = gix::open(tmp.path()).unwrap();
        let result = checkout(&mut repo, "v2.0.0");
        assert!(
            result.is_ok(),
            "checkout('v2.0.0') should succeed for tag: {:?}",
            result.err()
        );

        assert!(repo.head().unwrap().is_detached());
    }

    #[test]
    fn test_sync_branch_with_remote_tracking() {
        let (tmp, _repo) = create_test_repo_with_remote();
        Command::new("git")
            .args(["branch", "-f", "main"])
            .current_dir(tmp.path())
            .status()
            .unwrap();
        Command::new("git")
            .args(["checkout", "main"])
            .current_dir(tmp.path())
            .status()
            .unwrap();

        let mut repo = gix::open(tmp.path()).unwrap();
        let result = sync_branch(&mut repo);
        assert!(
            result.is_ok(),
            "sync_branch should succeed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_sync_branch_detached_head() {
        let (tmp, _repo) = create_test_repo();
        Command::new("git")
            .args(["checkout", "--detach", "HEAD"])
            .current_dir(tmp.path())
            .status()
            .unwrap();

        let mut repo = gix::open(tmp.path()).unwrap();
        let result = sync_branch(&mut repo);
        assert!(result.is_ok());
    }

    #[test]
    fn test_sync_branch_no_remote_ref() {
        let (tmp, _repo) = create_test_repo();
        let mut repo = gix::open(tmp.path()).unwrap();
        let result = sync_branch(&mut repo);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_checkout_async_with_remote_branch() {
        let (tmp, _repo) = create_test_repo();
        Command::new("git")
            .args(["remote", "add", "origin", "https://example.com/fake.git"])
            .current_dir(tmp.path())
            .status()
            .unwrap();
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        let commit_hash = String::from_utf8(output.stdout).unwrap().trim().to_string();
        Command::new("git")
            .args(["update-ref", "refs/remotes/origin/main", &commit_hash])
            .current_dir(tmp.path())
            .status()
            .unwrap();

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
        let (tmp, _repo) = create_test_repo();
        Command::new("git")
            .args(["remote", "add", "origin", "https://example.com/fake.git"])
            .current_dir(tmp.path())
            .status()
            .unwrap();
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        let commit_hash = String::from_utf8(output.stdout).unwrap().trim().to_string();
        Command::new("git")
            .args(["update-ref", "refs/remotes/origin/main", &commit_hash])
            .current_dir(tmp.path())
            .status()
            .unwrap();

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
        let (tmp, _repo) = create_test_repo();
        Command::new("git")
            .args(["remote", "add", "origin", "https://example.com/fake.git"])
            .current_dir(tmp.path())
            .status()
            .unwrap();
        Command::new("git")
            .args(["tag", "v3.0.0"])
            .current_dir(tmp.path())
            .status()
            .unwrap();

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
