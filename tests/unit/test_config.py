"""Tests for configuration module."""

from collections.abc import Callable
from pathlib import Path

import pytest

from cangjie_mcp.config import (
    IndexInfo,
    Settings,
    get_settings,
    reset_settings,
    set_settings,
)


class TestSettings:
    """Tests for Settings class."""

    def test_default_values(self) -> None:
        """Test that Settings has sensible defaults."""
        # Settings should be creatable with all defaults
        settings = Settings()
        assert settings.docs_version == "latest"
        assert settings.docs_lang == "zh"
        assert settings.embedding_type == "local"
        assert settings.rerank_type == "none"
        assert settings.data_dir.name == ".cangjie-mcp"

    def test_all_fields_provided(self, temp_data_dir: Path, create_test_settings: Callable[..., Settings]) -> None:
        """Test Settings with all required fields."""
        settings = create_test_settings(
            docs_version="v1.0.7",
            docs_lang="en",
            embedding_type="openai",
            data_dir=temp_data_dir,
        )
        assert settings.docs_version == "v1.0.7"
        assert settings.docs_lang == "en"
        assert settings.embedding_type == "openai"
        assert settings.data_dir == temp_data_dir

    def test_derived_paths(self, temp_data_dir: Path, create_test_settings: Callable[..., Settings]) -> None:
        """Test derived path properties."""
        settings = create_test_settings(data_dir=temp_data_dir, docs_lang="zh", docs_version="v1.0.7")
        assert settings.docs_repo_dir == temp_data_dir / "docs_repo"

    def test_version_isolation(self, temp_data_dir: Path, create_test_settings: Callable[..., Settings]) -> None:
        """Test that different versions share the same docs_repo."""
        settings_v1 = create_test_settings(data_dir=temp_data_dir, docs_version="v1.0.6", docs_lang="zh")
        settings_v2 = create_test_settings(data_dir=temp_data_dir, docs_version="v1.0.7", docs_lang="zh")

        # Both share the same docs_repo
        assert settings_v1.docs_repo_dir == settings_v2.docs_repo_dir


class TestIndexInfo:
    """Tests for IndexInfo frozen dataclass."""

    def test_from_settings(self, temp_data_dir: Path, create_test_settings: Callable[..., Settings]) -> None:
        """Test IndexInfo.from_settings constructs correctly."""
        settings = create_test_settings(data_dir=temp_data_dir, docs_version="v1.0.7", docs_lang="zh")
        index_info = IndexInfo.from_settings(settings)
        assert index_info.version == "v1.0.7"
        assert index_info.lang == "zh"
        assert index_info.embedding_model_name == "local:paraphrase-multilingual-MiniLM-L12-v2"
        assert index_info.data_dir == temp_data_dir

    def test_path_properties(self, temp_data_dir: Path) -> None:
        """Test derived path properties on IndexInfo."""
        index_info = IndexInfo(
            version="v1.0.7",
            lang="zh",
            embedding_model_name="local:paraphrase-multilingual-MiniLM-L12-v2",
            data_dir=temp_data_dir,
        )
        assert (
            index_info.index_dir
            == temp_data_dir / "indexes" / "v1.0.7" / "zh" / "local--paraphrase-multilingual-MiniLM-L12-v2"
        )
        assert (
            index_info.chroma_db_dir
            == temp_data_dir
            / "indexes"
            / "v1.0.7"
            / "zh"
            / "local--paraphrase-multilingual-MiniLM-L12-v2"
            / "chroma_db"
        )
        assert index_info.docs_repo_dir == temp_data_dir / "docs_repo"
        assert "source_zh_cn" in str(index_info.docs_source_dir)

        # Test English language
        index_info_en = IndexInfo(
            version="v1.0.7",
            lang="en",
            embedding_model_name="local:paraphrase-multilingual-MiniLM-L12-v2",
            data_dir=temp_data_dir,
        )
        assert (
            index_info_en.index_dir
            == temp_data_dir / "indexes" / "v1.0.7" / "en" / "local--paraphrase-multilingual-MiniLM-L12-v2"
        )
        assert "source_en" in str(index_info_en.docs_source_dir)

    def test_version_isolation(self, temp_data_dir: Path) -> None:
        """Test that different versions have separate index directories."""
        info_v1 = IndexInfo(
            version="v1.0.6",
            lang="zh",
            embedding_model_name="local:test",
            data_dir=temp_data_dir,
        )
        info_v2 = IndexInfo(
            version="v1.0.7",
            lang="zh",
            embedding_model_name="local:test",
            data_dir=temp_data_dir,
        )
        assert info_v1.index_dir != info_v2.index_dir
        assert info_v1.chroma_db_dir != info_v2.chroma_db_dir
        assert info_v1.docs_repo_dir == info_v2.docs_repo_dir

    def test_frozen(self, temp_data_dir: Path) -> None:
        """Test that IndexInfo is immutable."""
        index_info = IndexInfo(
            version="v1.0.7",
            lang="zh",
            embedding_model_name="local:test",
            data_dir=temp_data_dir,
        )
        with pytest.raises(AttributeError):
            index_info.version = "v2.0.0"  # type: ignore[misc]


class TestOpenAISettings:
    """Tests for OpenAI settings in unified Settings class."""

    def test_default_optional_values(self, create_test_settings: Callable[..., Settings]) -> None:
        """Test default OpenAI configuration (optional fields with defaults)."""
        settings = create_test_settings()
        assert settings.openai_api_key is None
        assert settings.openai_base_url == "https://api.openai.com/v1"
        assert settings.openai_model == "text-embedding-3-small"

    def test_custom_values(self, create_test_settings: Callable[..., Settings]) -> None:
        """Test custom OpenAI configuration."""
        settings = create_test_settings(
            openai_api_key="test-key",
            openai_base_url="https://custom.api.com/v1",
            openai_model="text-embedding-3-large",
        )
        assert settings.openai_api_key == "test-key"
        assert settings.openai_base_url == "https://custom.api.com/v1"
        assert settings.openai_model == "text-embedding-3-large"


class TestRerankSettings:
    """Tests for rerank settings."""

    def test_rerank_fields(self, create_test_settings: Callable[..., Settings]) -> None:
        """Test rerank configuration with create_test_settings defaults."""
        settings = create_test_settings()
        assert settings.rerank_type == "none"
        assert settings.rerank_model == "BAAI/bge-reranker-v2-m3"
        assert settings.rerank_top_k == 5
        assert settings.rerank_initial_k == 20

    def test_custom_values(self, create_test_settings: Callable[..., Settings]) -> None:
        """Test custom rerank configuration."""
        settings = create_test_settings(
            rerank_type="local",
            rerank_model="custom-model",
            rerank_top_k=10,
            rerank_initial_k=50,
        )
        assert settings.rerank_type == "local"
        assert settings.rerank_model == "custom-model"
        assert settings.rerank_top_k == 10
        assert settings.rerank_initial_k == 50


class TestGetSettings:
    """Tests for get_settings function."""

    def test_get_settings_raises_without_init(self) -> None:
        """Test that get_settings raises RuntimeError if not initialized."""
        reset_settings()
        with pytest.raises(RuntimeError, match="Settings not initialized"):
            get_settings()

    def test_get_settings_returns_set_settings(self, create_test_settings: Callable[..., Settings]) -> None:
        """Test that get_settings returns the set settings."""
        reset_settings()
        settings = create_test_settings(docs_version="v1.0.0")
        set_settings(settings)

        retrieved = get_settings()
        assert retrieved is settings
        assert retrieved.docs_version == "v1.0.0"

        reset_settings()


class TestSetSettings:
    """Tests for set_settings function."""

    def test_set_settings_updates_global(self, create_test_settings: Callable[..., Settings]) -> None:
        """Test that set_settings updates the global settings."""
        reset_settings()

        custom_settings = create_test_settings(docs_version="v2.0.0", docs_lang="en")
        set_settings(custom_settings)

        retrieved = get_settings()
        assert retrieved.docs_version == "v2.0.0"
        assert retrieved.docs_lang == "en"

        reset_settings()

    def test_reset_settings_clears_global(self, create_test_settings: Callable[..., Settings]) -> None:
        """Test that reset_settings clears the global settings."""
        custom_settings = create_test_settings(docs_version="v3.0.0")
        set_settings(custom_settings)

        reset_settings()

        # After reset, get_settings should raise
        with pytest.raises(RuntimeError, match="Settings not initialized"):
            get_settings()
