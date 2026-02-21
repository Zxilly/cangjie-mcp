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
    DEFAULT_RRF_K,
    get_default_data_dir,
)


def _sanitize_for_path(name: str) -> str:
    """Sanitize a string for use in file paths."""
    return name.replace(":", "--").replace("/", "--")


@dataclass(frozen=True)
class IndexInfo:
    """Identity and paths for the currently loaded index. Immutable after creation."""

    version: str
    lang: str
    embedding_model_name: str
    data_dir: Path

    @property
    def index_dir(self) -> Path:
        """Path to version-specific index directory."""
        model_dir = (
            "bm25-only" if self.embedding_model_name == "none" else _sanitize_for_path(self.embedding_model_name)
        )
        return self.data_dir / "indexes" / self.version / self.lang / model_dir

    @property
    def bm25_index_dir(self) -> Path:
        """Path to BM25 index directory."""
        return self.index_dir / "bm25_index"

    @property
    def chroma_db_dir(self) -> Path:
        """Path to ChromaDB database (version-specific)."""
        return self.index_dir / "chroma_db"

    @property
    def docs_repo_dir(self) -> Path:
        """Path to cloned documentation repository."""
        return self.data_dir / "docs_repo"

    @property
    def docs_source_dir(self) -> Path:
        """Path to documentation source based on language."""
        lang_dir = "source_zh_cn" if self.lang == "zh" else "source_en"
        return self.docs_repo_dir / "docs" / "dev-guide" / lang_dir

    @classmethod
    def from_settings(cls, settings: Settings) -> IndexInfo:
        """Construct IndexInfo from CLI settings."""
        return cls(
            version=settings.docs_version,
            lang=settings.docs_lang,
            embedding_model_name=settings.embedding_model_name,
            data_dir=settings.data_dir,
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
    embedding_type: Literal["none", "local", "openai"] = DEFAULT_EMBEDDING_TYPE
    local_model: str = DEFAULT_LOCAL_MODEL

    # Rerank settings
    rerank_type: Literal["none", "local", "openai"] = DEFAULT_RERANK_TYPE
    rerank_model: str = DEFAULT_RERANK_MODEL
    rerank_top_k: int = DEFAULT_RERANK_TOP_K
    rerank_initial_k: int = DEFAULT_RERANK_INITIAL_K

    # RRF settings
    rrf_k: int = DEFAULT_RRF_K

    # Chunking settings
    chunk_max_size: int = DEFAULT_CHUNK_MAX_SIZE

    # Data directory (use field with default_factory for mutable default)
    data_dir: Path = field(default_factory=get_default_data_dir)

    # Remote server URL (when set, forwards queries to an HTTP server)
    server_url: str | None = None

    # OpenAI-compatible API settings
    openai_api_key: str | None = None
    openai_base_url: str = DEFAULT_OPENAI_BASE_URL
    openai_model: str = DEFAULT_OPENAI_MODEL

    @property
    def has_embedding(self) -> bool:
        """Whether an embedding model is configured."""
        return self.embedding_type != "none"

    @property
    def embedding_model_name(self) -> str:
        """Return a canonical name for the current embedding model."""
        if self.embedding_type == "none":
            return "none"
        if self.embedding_type == "local":
            return f"local:{self.local_model}"
        return f"openai:{self.openai_model}"

    @property
    def docs_repo_dir(self) -> Path:
        """Path to cloned documentation repository."""
        return self.data_dir / "docs_repo"


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


def format_startup_info(settings: Settings, index_info: IndexInfo) -> str:
    """Format startup configuration and index info for display.

    Returns a multi-line string with embedding/rerank settings and
    index metadata, suitable for logging or terminal output.
    """
    from cangjie_mcp import __version__

    lines: list[str] = [""]
    lines.append(f"  Cangjie MCP v{__version__}")
    lines.append("  ┌─ Configuration ─────────────────────────────")

    if settings.server_url:
        lines.append(f"  │ Mode       : remote → {settings.server_url}")
    else:
        search_mode = "hybrid (BM25 + vector)" if settings.has_embedding else "BM25"
        lines.append(f"  │ Search     : {search_mode}")
        if settings.has_embedding:
            model = settings.local_model if settings.embedding_type == "local" else settings.openai_model
            lines.append(f"  │ Embedding  : {settings.embedding_type} · {model}")

    if settings.rerank_type == "none":
        lines.append("  │ Rerank     : disabled")
    else:
        lines.append(f"  │ Rerank     : {settings.rerank_type} · {settings.rerank_model}")
        lines.append(f"  │              top_k={settings.rerank_top_k}  initial_k={settings.rerank_initial_k}")

    lines.append("  ├─ Index ──────────────────────────────────────")
    lines.append(f"  │ Version    : {index_info.version}")
    lines.append(f"  │ Language   : {index_info.lang}")
    if settings.has_embedding:
        lines.append(f"  │ Model      : {index_info.embedding_model_name}")

    if not settings.server_url:
        lines.append(f"  │ Data Dir   : {index_info.data_dir}")
        lines.append(f"  │ Index Dir  : {index_info.index_dir}")
        if settings.has_embedding:
            lines.append(f"  │ ChromaDB   : {index_info.chroma_db_dir}")

    lines.append("  └─────────────────────────────────────────────")
    lines.append("")

    return "\n".join(lines)
