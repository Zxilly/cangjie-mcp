"""Git repository management for Cangjie documentation."""

from pathlib import Path

from git import GitCommandError, Repo

from cangjie_mcp.utils import console

DOCS_REPO_URL = "https://gitcode.com/Cangjie/cangjie_docs.git"


class GitManager:
    """Manages the Cangjie documentation git repository."""

    def __init__(self, repo_dir: Path) -> None:
        """Initialize git manager.

        Args:
            repo_dir: Path to store/access the cloned repository
        """
        self.repo_dir = repo_dir
        self._repo: Repo | None = None

    @property
    def repo(self) -> Repo | None:
        """Get the git repository if it exists."""
        if self._repo is None and self.repo_dir.exists():
            try:
                self._repo = Repo(self.repo_dir)
            except Exception:
                self._repo = None
        return self._repo

    def is_cloned(self) -> bool:
        """Check if repository is already cloned."""
        return self.repo_dir.exists() and (self.repo_dir / ".git").exists()

    def clone(self) -> Repo:
        """Clone the documentation repository.

        Returns:
            The cloned repository
        """
        console.print(f"[blue]Cloning repository from {DOCS_REPO_URL}...[/blue]")
        self.repo_dir.parent.mkdir(parents=True, exist_ok=True)
        self._repo = Repo.clone_from(DOCS_REPO_URL, self.repo_dir)
        console.print("[green]Repository cloned successfully.[/green]")
        return self._repo

    def ensure_cloned(self, fetch: bool = True) -> Repo:
        """Ensure repository is cloned and up-to-date.

        If the repository already exists, fetches all tags and commits
        from remote to ensure we have the latest versions available.

        Args:
            fetch: Whether to fetch from remote if repo exists (default: True)

        Returns:
            The repository
        """
        if self.is_cloned():
            repo = self.repo
            if repo is not None:
                if fetch:
                    self._fetch_all(repo)
                return repo
        return self.clone()

    def _fetch_all(self, repo: Repo) -> None:
        """Fetch all tags and commits from remote.

        Args:
            repo: The git repository
        """
        console.print("[blue]Fetching latest tags and commits...[/blue]")
        try:
            # Fetch all branches and tags
            repo.remotes.origin.fetch(tags=True, prune=True)
            console.print("[green]Fetch complete.[/green]")
        except GitCommandError as e:
            console.print(f"[yellow]Warning: Failed to fetch from remote: {e}[/yellow]")

    def list_tags(self) -> list[str]:
        """List all available tags in the repository.

        Returns:
            List of tag names sorted by version
        """
        repo = self.ensure_cloned()
        tags = [tag.name for tag in repo.tags]
        return sorted(tags, reverse=True)

    def get_current_version(self) -> str | None:
        """Get current checked out version (tag or branch).

        Returns:
            Current tag name if on a tag, branch name otherwise, None if detached
        """
        repo = self.repo
        if repo is None:
            return None

        try:
            # Check if HEAD matches any tag
            head_commit = repo.head.commit
            for tag in repo.tags:
                if tag.commit == head_commit:
                    return tag.name

            # Return branch name if not detached
            if not repo.head.is_detached:
                return repo.active_branch.name
        except Exception:
            pass

        return None

    def checkout(self, version: str) -> None:
        """Checkout a specific version (tag or branch).

        Args:
            version: Tag name or branch name to checkout
        """
        repo = self.ensure_cloned()

        # Handle "latest" - try main, then master
        if version == "latest":
            for branch in ("main", "master"):
                try:
                    repo.git.checkout(branch)
                    console.print(f"[green]Checked out {branch} branch.[/green]")
                    return
                except GitCommandError:
                    continue
            # Fall through to try "latest" as a literal tag/branch name

        # Try to checkout the specified version
        try:
            repo.git.checkout(version)
            console.print(f"[green]Checked out version {version}.[/green]")
        except GitCommandError as e:
            raise ValueError(f"Failed to checkout version '{version}': {e}") from e

    def fetch(self) -> None:
        """Fetch latest changes from remote."""
        repo = self.ensure_cloned(fetch=False)
        self._fetch_all(repo)

    def pull(self) -> None:
        """Pull latest changes (for branches, not tags)."""
        repo = self.ensure_cloned(fetch=False)
        self._fetch_all(repo)
        if not repo.head.is_detached:
            console.print("[blue]Pulling latest changes...[/blue]")
            repo.remotes.origin.pull()
            console.print("[green]Pull complete.[/green]")
