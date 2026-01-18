"""Tests for configuration module."""

from pathlib import Path

from cangjie_mcp.config import OpenAISettings, Settings


class TestSettings:
    """Tests for Settings class."""

    def test_default_values(self) -> None:
        """Test default configuration values."""
        settings = Settings()
        assert settings.docs_version == "latest"
        assert settings.docs_lang == "zh"
        assert settings.embedding_type == "local"
        assert settings.local_model == "paraphrase-multilingual-MiniLM-L12-v2"
        assert settings.data_dir == Path("./data")

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
        settings = Settings(data_dir=temp_data_dir, docs_lang="zh")
        assert settings.docs_repo_dir == temp_data_dir / "docs_repo"
        assert settings.chroma_db_dir == temp_data_dir / "chroma_db"
        assert "source_zh_cn" in str(settings.docs_source_dir)

        settings_en = Settings(data_dir=temp_data_dir, docs_lang="en")
        assert "source_en" in str(settings_en.docs_source_dir)


class TestOpenAISettings:
    """Tests for OpenAI settings."""

    def test_default_values(self) -> None:
        """Test default OpenAI configuration."""
        settings = OpenAISettings()
        assert settings.api_key is None
        assert settings.base_url == "https://api.openai.com/v1"
        assert settings.model == "text-embedding-3-small"
