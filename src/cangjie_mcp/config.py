"""Configuration management for Cangjie MCP.

All configuration is managed through CLI arguments, which can be set via
environment variables using Typer's envvar feature.

Run `cangjie-mcp --help` to see all available options and their environment variables.
"""

from dataclasses import dataclass
from pathlib import Path
from typing import Literal

from pydantic import BaseModel, Field


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
    def from_string(cls, s: str) -> "IndexKey":
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
    def parse_list(cls, indexes_str: str) -> list["IndexKey"]:
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


def _get_default_data_dir() -> Path:
    """Get the default data directory (~/.cangjie-mcp)."""
    return Path.home() / ".cangjie-mcp"


class Settings(BaseModel):
    """Application settings.

    All settings are configured via CLI arguments (with environment variable support).
    Use `cangjie-mcp --help` to see all options and their environment variables.
    """

    # Documentation settings
    docs_version: str = Field(default="latest", description="Documentation version (git tag)")
    docs_lang: Literal["zh", "en"] = Field(default="zh", description="Documentation language")

    # Embedding settings
    embedding_type: Literal["local", "openai"] = Field(default="local", description="Embedding model type")
    local_model: str = Field(
        default="paraphrase-multilingual-MiniLM-L12-v2",
        description="Local HuggingFace embedding model name",
    )

    # Rerank settings
    rerank_type: Literal["none", "local", "openai"] = Field(
        default="none", description="Reranker type (none/local/openai)"
    )
    rerank_model: str = Field(
        default="BAAI/bge-reranker-v2-m3",
        description="Rerank model name (used for both local and OpenAI-compatible reranking)",
    )
    rerank_top_k: int = Field(default=5, description="Number of results to return after reranking")
    rerank_initial_k: int = Field(default=20, description="Number of candidates to retrieve before reranking")

    # Chunking settings
    chunk_max_size: int = Field(
        default=6000,
        description="Max chunk size in chars to prevent exceeding embedding token limits",
    )

    # Data directory (default: ~/.cangjie-mcp)
    data_dir: Path = Field(
        default_factory=_get_default_data_dir,
        description="Data directory path",
    )

    # Prebuilt index URL
    prebuilt_url: str | None = Field(default=None, description="Prebuilt index download URL")

    # OpenAI-compatible API settings
    openai_api_key: str | None = Field(
        default=None,
        description="OpenAI-compatible API Key",
    )
    openai_base_url: str = Field(
        default="https://api.openai.com/v1",
        description="OpenAI-compatible API Base URL",
    )
    openai_model: str = Field(
        default="text-embedding-3-small",
        description="OpenAI-compatible embedding model",
    )

    # HTTP server settings (for serve command)
    http_host: str = Field(
        default="127.0.0.1",
        description="HTTP server host address",
    )
    http_port: int = Field(
        default=8000,
        description="HTTP server port",
    )

    # Multi-index settings (for HTTP mode)
    indexes: str | None = Field(
        default=None,
        description="Comma-separated list of index URLs to load",
    )

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

    Returns the global settings if set, otherwise returns default settings.
    CLI commands should call set_settings() to initialize with CLI values.
    """
    global _settings
    if _settings is None:
        _settings = Settings()
    return _settings


def set_settings(settings: Settings) -> None:
    """Set the global settings instance."""
    global _settings
    _settings = settings


def reset_settings() -> None:
    """Reset the global settings instance (useful for testing)."""
    global _settings
    _settings = None
