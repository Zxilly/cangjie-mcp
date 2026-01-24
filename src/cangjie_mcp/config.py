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
    DEFAULT_HTTP_HOST,
    DEFAULT_HTTP_PORT,
    DEFAULT_LOCAL_MODEL,
    DEFAULT_OPENAI_BASE_URL,
    DEFAULT_OPENAI_MODEL,
    DEFAULT_RERANK_INITIAL_K,
    DEFAULT_RERANK_MODEL,
    DEFAULT_RERANK_TOP_K,
    DEFAULT_RERANK_TYPE,
)


@dataclass(frozen=True)
class IndexKey:
    """Index identifier for multi-index support.

    An IndexKey uniquely identifies a documentation index by version and language.
    It is hashable and can be used as a dictionary key.
    """

    version: str
    lang: str

    def __str__(self) -> str:
        """Return string representation as 'version:lang'."""
        return f"{self.version}:{self.lang}"

    def __repr__(self) -> str:
        """Return repr string."""
        return f"IndexKey(version={self.version!r}, lang={self.lang!r})"

    @classmethod
    def from_string(cls, s: str) -> IndexKey:
        """Parse IndexKey from 'version:lang' string format.

        Args:
            s: String in 'version:lang' format (e.g., 'v1:zh', 'latest:en')

        Returns:
            IndexKey instance

        Raises:
            ValueError: If string format is invalid
        """
        parts = s.split(":")
        if len(parts) != 2:
            raise ValueError(f"Invalid IndexKey format: '{s}'. Expected 'version:lang' (e.g., 'v1:zh')")
        version, lang = parts
        if not version or not lang:
            raise ValueError(f"Invalid IndexKey format: '{s}'. Version and lang cannot be empty")
        return cls(version=version.strip(), lang=lang.strip())

    @classmethod
    def parse_list(cls, indexes_str: str) -> list[IndexKey]:
        """Parse comma-separated list of index keys.

        Args:
            indexes_str: Comma-separated string (e.g., 'v1:zh,latest:en')

        Returns:
            List of IndexKey instances
        """
        if not indexes_str or not indexes_str.strip():
            return []
        return [cls.from_string(s.strip()) for s in indexes_str.split(",") if s.strip()]

    @property
    def path_segment(self) -> str:
        """Return path segment for URL routing (e.g., 'v1/zh')."""
        return f"{self.version}/{self.lang}"


def _default_data_dir() -> Path:
    """Get the default data directory (~/.cangjie-mcp)."""
    return Path.home() / ".cangjie-mcp"


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
    data_dir: Path = field(default_factory=_default_data_dir)

    # Prebuilt index URL
    prebuilt_url: str | None = None

    # OpenAI-compatible API settings
    openai_api_key: str | None = None
    openai_base_url: str = DEFAULT_OPENAI_BASE_URL
    openai_model: str = DEFAULT_OPENAI_MODEL

    # HTTP server settings (for serve command)
    http_host: str = DEFAULT_HTTP_HOST
    http_port: int = DEFAULT_HTTP_PORT

    # Multi-index settings (for HTTP mode)
    indexes: str | None = None

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
