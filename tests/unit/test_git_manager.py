"""Tests for repo/git_manager.py Git repository management."""

from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

from cangjie_mcp.repo.git_manager import DOCS_REPO_URL, GitManager


class TestGitManagerBasic:
    """Basic tests for GitManager that don't require network."""

    def test_init(self, temp_data_dir: Path) -> None:
        """Test GitManager initialization."""
        repo_dir = temp_data_dir / "docs_repo"
        manager = GitManager(repo_dir)

        assert manager.repo_dir == repo_dir
        assert manager._repo is None

    def test_is_cloned_false(self, temp_data_dir: Path) -> None:
        """Test is_cloned returns False when repo doesn't exist."""
        repo_dir = temp_data_dir / "docs_repo"
        manager = GitManager(repo_dir)

        assert manager.is_cloned() is False

    def test_is_cloned_partial(self, temp_data_dir: Path) -> None:
        """Test is_cloned returns False when directory exists but no .git."""
        repo_dir = temp_data_dir / "docs_repo"
        repo_dir.mkdir(parents=True)
        manager = GitManager(repo_dir)

        assert manager.is_cloned() is False

    def test_repo_property_no_repo(self, temp_data_dir: Path) -> None:
        """Test repo property returns None when repo doesn't exist."""
        repo_dir = temp_data_dir / "docs_repo"
        manager = GitManager(repo_dir)

        assert manager.repo is None

    def test_get_current_version_no_repo(self, temp_data_dir: Path) -> None:
        """Test get_current_version returns None when no repo."""
        repo_dir = temp_data_dir / "docs_repo"
        manager = GitManager(repo_dir)

        assert manager.get_current_version() is None


class TestGitManagerWithMockRepo:
    """Tests for GitManager using mocked Git repository."""

    @pytest.fixture
    def mock_repo(self) -> MagicMock:
        """Create a mock Git repository."""
        repo = MagicMock()
        repo.tags = []
        repo.head.is_detached = False
        repo.head.commit = MagicMock()
        repo.active_branch.name = "main"
        repo.remotes.origin = MagicMock()
        repo.git = MagicMock()
        return repo

    @pytest.fixture
    def manager_with_mock_repo(self, temp_data_dir: Path, mock_repo: MagicMock) -> GitManager:
        """Create a GitManager with a mocked repository."""
        repo_dir = temp_data_dir / "docs_repo"
        repo_dir.mkdir(parents=True)
        (repo_dir / ".git").mkdir()

        manager = GitManager(repo_dir)
        manager._repo = mock_repo
        return manager

    def test_is_cloned_true(self, manager_with_mock_repo: GitManager) -> None:
        """Test is_cloned returns True when .git exists."""
        assert manager_with_mock_repo.is_cloned() is True

    def test_list_tags(self, manager_with_mock_repo: GitManager, mock_repo: MagicMock) -> None:
        """Test listing tags."""
        tag1 = MagicMock()
        tag1.name = "v1.0.7"
        tag2 = MagicMock()
        tag2.name = "v0.52.0"
        mock_repo.tags = [tag1, tag2]

        tags = manager_with_mock_repo.list_tags()
        assert "v1.0.7" in tags
        assert "v0.52.0" in tags

    def test_get_current_version_on_branch(self, manager_with_mock_repo: GitManager, mock_repo: MagicMock) -> None:
        """Test get_current_version when on a branch."""
        mock_repo.head.is_detached = False
        mock_repo.active_branch.name = "main"
        mock_repo.tags = []

        version = manager_with_mock_repo.get_current_version()
        assert version == "main"

    def test_get_current_version_on_tag(self, manager_with_mock_repo: GitManager, mock_repo: MagicMock) -> None:
        """Test get_current_version when HEAD matches a tag."""
        head_commit = MagicMock()
        mock_repo.head.commit = head_commit

        tag = MagicMock()
        tag.name = "v1.0.7"
        tag.commit = head_commit
        mock_repo.tags = [tag]

        version = manager_with_mock_repo.get_current_version()
        assert version == "v1.0.7"

    def test_checkout_latest(self, manager_with_mock_repo: GitManager, mock_repo: MagicMock) -> None:
        """Test checkout 'latest' tries main then master."""
        manager_with_mock_repo.checkout("latest")
        mock_repo.git.checkout.assert_called_with("main")

    def test_checkout_version(self, manager_with_mock_repo: GitManager, mock_repo: MagicMock) -> None:
        """Test checkout specific version."""
        manager_with_mock_repo.checkout("v1.0.7")
        mock_repo.git.checkout.assert_called_with("v1.0.7")

    def test_fetch(self, manager_with_mock_repo: GitManager, mock_repo: MagicMock) -> None:
        """Test fetch from remote."""
        manager_with_mock_repo.fetch()
        mock_repo.remotes.origin.fetch.assert_called_once_with(tags=True, prune=True)

    def test_pull_on_branch(self, manager_with_mock_repo: GitManager, mock_repo: MagicMock) -> None:
        """Test pull when on a branch."""
        mock_repo.head.is_detached = False
        manager_with_mock_repo.pull()
        mock_repo.remotes.origin.pull.assert_called_once()

    def test_pull_detached_head(self, manager_with_mock_repo: GitManager, mock_repo: MagicMock) -> None:
        """Test pull when HEAD is detached (no pull)."""
        mock_repo.head.is_detached = True
        manager_with_mock_repo.pull()
        mock_repo.remotes.origin.pull.assert_not_called()


class TestGitManagerClone:
    """Tests for GitManager clone functionality using mocks."""

    @patch("cangjie_mcp.repo.git_manager.Repo")
    def test_clone(self, mock_repo_class: MagicMock, temp_data_dir: Path) -> None:
        """Test cloning a repository."""
        mock_cloned_repo = MagicMock()
        mock_repo_class.clone_from.return_value = mock_cloned_repo

        repo_dir = temp_data_dir / "docs_repo"
        manager = GitManager(repo_dir)

        result = manager.clone()

        mock_repo_class.clone_from.assert_called_once_with(DOCS_REPO_URL, repo_dir)
        assert result == mock_cloned_repo
        assert manager._repo == mock_cloned_repo

    @patch("cangjie_mcp.repo.git_manager.Repo")
    def test_ensure_cloned_when_not_cloned(self, mock_repo_class: MagicMock, temp_data_dir: Path) -> None:
        """Test ensure_cloned clones when repo doesn't exist."""
        mock_cloned_repo = MagicMock()
        mock_repo_class.clone_from.return_value = mock_cloned_repo

        repo_dir = temp_data_dir / "docs_repo"
        manager = GitManager(repo_dir)

        result = manager.ensure_cloned()

        mock_repo_class.clone_from.assert_called_once()
        assert result == mock_cloned_repo

    def test_ensure_cloned_when_already_cloned(self, temp_data_dir: Path) -> None:
        """Test ensure_cloned returns existing repo and fetches."""
        repo_dir = temp_data_dir / "docs_repo"
        repo_dir.mkdir(parents=True)
        (repo_dir / ".git").mkdir()

        mock_repo = MagicMock()
        manager = GitManager(repo_dir)
        manager._repo = mock_repo

        result = manager.ensure_cloned()
        assert result == mock_repo
        # Verify fetch was called
        mock_repo.remotes.origin.fetch.assert_called_once_with(tags=True, prune=True)

    def test_ensure_cloned_without_fetch(self, temp_data_dir: Path) -> None:
        """Test ensure_cloned with fetch=False skips fetching."""
        repo_dir = temp_data_dir / "docs_repo"
        repo_dir.mkdir(parents=True)
        (repo_dir / ".git").mkdir()

        mock_repo = MagicMock()
        manager = GitManager(repo_dir)
        manager._repo = mock_repo

        result = manager.ensure_cloned(fetch=False)
        assert result == mock_repo
        # Verify fetch was NOT called
        mock_repo.remotes.origin.fetch.assert_not_called()


class TestDocsRepoUrl:
    """Test for the DOCS_REPO_URL constant."""

    def test_docs_repo_url(self) -> None:
        """Test that DOCS_REPO_URL is defined correctly."""
        assert DOCS_REPO_URL == "https://gitcode.com/Cangjie/cangjie_docs.git"
        assert DOCS_REPO_URL.startswith("https://")
        assert DOCS_REPO_URL.endswith(".git")
