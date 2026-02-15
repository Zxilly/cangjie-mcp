"""Configuration management for Cangjie MCP.

All configuration is managed through CLI arguments, which can be set via
environment variables using Typer's envvar feature.

Run `cangjie-mcp --help` to see all available options and their environment variables.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Literal

from cangjie_mcp.defaults import (
    DEFAULT_CHUNK_MAX_SIZE,
    DEFAULT_DOCS_LANG,
    DEFAULT_DOCS_VERSION,
    DEFAULT_EMBEDDING_TYPE,
    DEFAULT_LOCAL_MODEL,
    DEFAULT_OPENAI_BASE_URL,
    DEFAULT_OPENAI_MODEL,
    DEFAULT_RERANK_INITIAL_K,
    DEFAULT_RERANK_MODEL,
    DEFAULT_RERANK_TOP_K,
    DEFAULT_RERANK_TYPE,
    get_default_data_dir,
)


@dataclass
class Settings:
    """Application settings.

    All settings are configured via CLI arguments (with environment variable support).
    Use `cangjie-mcp --help` to see all options and their environment variables.

    Default values are imported from cangjie_mcp.defaults module.
    """

    # Documentation settings
    docs_version: str = DEFAULT_DOCS_VERSION
    docs_lang: Literal["zh", "en"] = DEFAULT_DOCS_LANG

    # Embedding settings
    embedding_type: Literal["local", "openai"] = DEFAULT_EMBEDDING_TYPE
    local_model: str = DEFAULT_LOCAL_MODEL

    # Rerank settings
    rerank_type: Literal["none", "local", "openai"] = DEFAULT_RERANK_TYPE
    rerank_model: str = DEFAULT_RERANK_MODEL
    rerank_top_k: int = DEFAULT_RERANK_TOP_K
    rerank_initial_k: int = DEFAULT_RERANK_INITIAL_K

    # Chunking settings
    chunk_max_size: int = DEFAULT_CHUNK_MAX_SIZE

    # Data directory (use field with default_factory for mutable default)
    data_dir: Path = field(default_factory=get_default_data_dir)

    # Prebuilt index URL
    prebuilt_url: str | None = None

    # OpenAI-compatible API settings
    openai_api_key: str | None = None
    openai_base_url: str = DEFAULT_OPENAI_BASE_URL
    openai_model: str = DEFAULT_OPENAI_MODEL

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


# Global settings instance
_settings: Settings | None = None


def get_settings() -> Settings:
    """Get settings instance.

    Returns the global settings. Raises RuntimeError if not initialized.
    CLI commands must call set_settings() before using this function.
    """
    if _settings is None:
        raise RuntimeError("Settings not initialized. Call set_settings() first.")
    return _settings


def set_settings(settings: Settings) -> None:
    """Set the global settings instance."""
    global _settings
    _settings = settings


def reset_settings() -> None:
    """Reset the global settings instance (useful for testing)."""
    global _settings
    _settings = None
