"""Tests for configuration module."""

from pathlib import Path
from unittest.mock import patch

import pytest

from cangjie_mcp.config import (
    Settings,
    get_settings,
    update_settings,
)


class TestSettings:
    """Tests for Settings class."""

    def test_default_values(self, monkeypatch: pytest.MonkeyPatch) -> None:
        """Test default configuration values."""
        # Clear environment variables to test defaults
        monkeypatch.delenv("CANGJIE_DATA_DIR", raising=False)
        monkeypatch.delenv("CANGJIE_DOCS_VERSION", raising=False)
        monkeypatch.delenv("CANGJIE_DOCS_LANG", raising=False)
        monkeypatch.delenv("CANGJIE_EMBEDDING_TYPE", raising=False)
        monkeypatch.delenv("CANGJIE_LOCAL_MODEL", raising=False)

        # Bypass .env file by passing _env_file=None
        settings = Settings(_env_file=None)  # type: ignore[call-arg]
        assert settings.docs_version == "latest"
        assert settings.docs_lang == "zh"
        assert settings.embedding_type == "local"
        assert settings.local_model == "paraphrase-multilingual-MiniLM-L12-v2"
        assert settings.data_dir == Path.home() / ".cangjie-mcp"

    def test_custom_values(self, temp_data_dir: Path) -> None:
        """Test custom configuration values."""
        settings = Settings(
            docs_version="v1.0.7",
            docs_lang="en",
            embedding_type="openai",
            data_dir=temp_data_dir,
        )
        assert settings.docs_version == "v1.0.7"
        assert settings.docs_lang == "en"
        assert settings.embedding_type == "openai"
        assert settings.data_dir == temp_data_dir

    def test_derived_paths(self, temp_data_dir: Path) -> None:
        """Test derived path properties."""
        settings = Settings(data_dir=temp_data_dir, docs_lang="zh", docs_version="v1.0.7")
        assert settings.docs_repo_dir == temp_data_dir / "docs_repo"
        assert settings.index_dir == temp_data_dir / "indexes" / "v1.0.7-zh"
        assert settings.chroma_db_dir == temp_data_dir / "indexes" / "v1.0.7-zh" / "chroma_db"
        assert "source_zh_cn" in str(settings.docs_source_dir)

        settings_en = Settings(data_dir=temp_data_dir, docs_lang="en", docs_version="v1.0.7")
        assert settings_en.index_dir == temp_data_dir / "indexes" / "v1.0.7-en"
        assert "source_en" in str(settings_en.docs_source_dir)

    def test_version_isolation(self, temp_data_dir: Path) -> None:
        """Test that different versions have separate index directories."""
        settings_v1 = Settings(data_dir=temp_data_dir, docs_version="v1.0.6", docs_lang="zh")
        settings_v2 = Settings(data_dir=temp_data_dir, docs_version="v1.0.7", docs_lang="zh")

        # Different versions should have different index directories
        assert settings_v1.index_dir != settings_v2.index_dir
        assert settings_v1.chroma_db_dir != settings_v2.chroma_db_dir

        # But share the same docs_repo
        assert settings_v1.docs_repo_dir == settings_v2.docs_repo_dir


class TestOpenAISettings:
    """Tests for OpenAI settings in unified Settings class."""

    def test_default_values(self, monkeypatch: pytest.MonkeyPatch) -> None:
        """Test default OpenAI configuration without environment variables."""
        # Clear environment variables to test defaults
        monkeypatch.delenv("OPENAI_API_KEY", raising=False)
        monkeypatch.delenv("OPENAI_BASE_URL", raising=False)
        monkeypatch.delenv("OPENAI_MODEL", raising=False)

        # Bypass .env file by passing _env_file=None
        settings = Settings(_env_file=None)  # type: ignore[call-arg]
        assert settings.openai_api_key is None
        assert settings.openai_base_url == "https://api.openai.com/v1"
        assert settings.openai_model == "text-embedding-3-small"

    def test_custom_values(self, monkeypatch: pytest.MonkeyPatch) -> None:
        """Test custom OpenAI configuration."""
        # Clear environment variables to ensure test values are used
        monkeypatch.delenv("OPENAI_API_KEY", raising=False)
        monkeypatch.delenv("OPENAI_BASE_URL", raising=False)
        monkeypatch.delenv("OPENAI_MODEL", raising=False)

        settings = Settings(
            openai_api_key="test-key",
            openai_base_url="https://custom.api.com/v1",
            openai_model="text-embedding-3-large",
            _env_file=None,  # type: ignore[call-arg]
        )
        assert settings.openai_api_key == "test-key"
        assert settings.openai_base_url == "https://custom.api.com/v1"
        assert settings.openai_model == "text-embedding-3-large"


class TestGetSettings:
    """Tests for get_settings function."""

    def test_get_settings_returns_settings(self) -> None:
        """Test that get_settings returns a Settings instance."""
        with patch("cangjie_mcp.config._settings", None):
            settings = get_settings()
            assert isinstance(settings, Settings)

    def test_get_settings_caches_instance(self) -> None:
        """Test that get_settings caches the settings instance."""
        with patch("cangjie_mcp.config._settings", None):
            settings1 = get_settings()
            settings2 = get_settings()
            assert settings1 is settings2


class TestUpdateSettings:
    """Tests for update_settings function."""

    def test_update_settings_changes_values(self) -> None:
        """Test that update_settings changes setting values."""
        with patch("cangjie_mcp.config._settings", None):
            original = get_settings()
            original_version = original.docs_version

            updated = update_settings(docs_version="v1.0.0")

            assert updated.docs_version == "v1.0.0"
            assert updated.docs_version != original_version

    def test_update_settings_ignores_none(self) -> None:
        """Test that update_settings ignores None values."""
        with patch("cangjie_mcp.config._settings", None):
            original = get_settings()
            original_version = original.docs_version

            updated = update_settings(docs_version=None)

            assert updated.docs_version == original_version

    def test_update_settings_multiple_values(self, temp_data_dir: Path) -> None:
        """Test updating multiple settings at once."""
        with patch("cangjie_mcp.config._settings", None):
            updated = update_settings(
                docs_version="v2.0.0",
                docs_lang="en",
                data_dir=temp_data_dir,
            )

            assert updated.docs_version == "v2.0.0"
            assert updated.docs_lang == "en"
            assert updated.data_dir == temp_data_dir
