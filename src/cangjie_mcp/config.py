"""Configuration management using pydantic-settings."""

from pathlib import Path
from typing import Literal

from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict


def _get_default_data_dir() -> Path:
    """Get the default data directory (~/.cangjie-mcp)."""
    return Path.home() / ".cangjie-mcp"


class Settings(BaseSettings):
    """Application settings loaded from environment variables."""

    model_config = SettingsConfigDict(
        env_prefix="CANGJIE_",
        env_file=".env",
        env_file_encoding="utf-8",
        extra="ignore",
    )

    # Documentation settings
    docs_version: str = Field(default="latest", description="Documentation version (git tag)")
    docs_lang: Literal["zh", "en"] = Field(default="zh", description="Documentation language")

    # Embedding settings
    embedding_type: Literal["local", "openai"] = Field(
        default="local", description="Embedding model type"
    )
    local_model: str = Field(
        default="paraphrase-multilingual-MiniLM-L12-v2",
        description="Local HuggingFace model name",
    )

    # Data directory (default: ~/.cangjie-mcp)
    data_dir: Path = Field(
        default_factory=_get_default_data_dir,
        description="Data directory path",
    )

    # Prebuilt index URL
    prebuilt_url: str | None = Field(default=None, description="Prebuilt index download URL")

    @property
    def docs_repo_dir(self) -> Path:
        """Path to cloned documentation repository."""
        return self.data_dir / "docs_repo"

    @property
    def index_dir(self) -> Path:
        """Path to version-specific index directory.

        Indexes are separated by version and language to prevent pollution.
        Example: ~/.cangjie-mcp/indexes/v1.0.7-zh/
        """
        return self.data_dir / "indexes" / f"{self.docs_version}-{self.docs_lang}"

    @property
    def chroma_db_dir(self) -> Path:
        """Path to ChromaDB database (version-specific)."""
        return self.index_dir / "chroma_db"

    @property
    def docs_source_dir(self) -> Path:
        """Path to documentation source based on language."""
        lang_dir = "source_zh_cn" if self.docs_lang == "zh" else "source_en"
        return self.docs_repo_dir / "docs" / "dev-guide" / lang_dir


class OpenAISettings(BaseSettings):
    """OpenAI-specific settings."""

    model_config = SettingsConfigDict(
        env_prefix="OPENAI_",
        env_file=".env",
        env_file_encoding="utf-8",
        extra="ignore",
    )

    api_key: str | None = Field(default=None, description="OpenAI API Key")
    base_url: str = Field(
        default="https://api.openai.com/v1", description="OpenAI API Base URL"
    )
    model: str = Field(
        default="text-embedding-3-small", description="OpenAI embedding model"
    )


# Global settings instances
_settings: Settings | None = None
_openai_settings: OpenAISettings | None = None


def get_settings() -> Settings:
    """Get or create settings instance."""
    global _settings
    if _settings is None:
        _settings = Settings()
    return _settings


def get_openai_settings() -> OpenAISettings:
    """Get or create OpenAI settings instance."""
    global _openai_settings
    if _openai_settings is None:
        _openai_settings = OpenAISettings()
    return _openai_settings


def update_settings(**kwargs: str | Path | None) -> Settings:
    """Update settings with new values (used by CLI to override env vars)."""
    global _settings
    current = get_settings()
    new_values = current.model_dump()
    new_values.update({k: v for k, v in kwargs.items() if v is not None})
    _settings = Settings(**new_values)
    return _settings
