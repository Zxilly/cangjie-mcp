"""Configuration management using pydantic-settings.

All configuration options can be set via:
1. Environment variables (with CANGJIE_ prefix for main settings)
2. .env file
3. CLI arguments (which override all other sources)

Environment variable mapping:
- CANGJIE_DOCS_VERSION -> docs_version
- CANGJIE_DOCS_LANG -> docs_lang
- CANGJIE_EMBEDDING_TYPE -> embedding_type
- CANGJIE_LOCAL_MODEL -> local_model
- CANGJIE_RERANK_TYPE -> rerank_type (none/local/openai)
- CANGJIE_RERANK_MODEL -> rerank_model
- CANGJIE_RERANK_TOP_K -> rerank_top_k
- CANGJIE_RERANK_INITIAL_K -> rerank_initial_k
- CANGJIE_DATA_DIR -> data_dir
- CANGJIE_PREBUILT_URL -> prebuilt_url
- CANGJIE_INDEXES -> indexes (for multi-index HTTP mode)
- CANGJIE_HTTP_HOST -> http_host
- CANGJIE_HTTP_PORT -> http_port
- OPENAI_API_KEY -> openai_api_key
- OPENAI_BASE_URL -> openai_base_url
- OPENAI_MODEL -> openai_model (for embeddings)

Note: SiliconFlow and other OpenAI-compatible APIs can be used by setting
OPENAI_BASE_URL to the provider's endpoint (e.g., https://api.siliconflow.cn/v1).
"""

from dataclasses import dataclass
from pathlib import Path
from typing import Literal

from pydantic import AliasChoices, Field
from pydantic_settings import BaseSettings, SettingsConfigDict


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
            raise ValueError(
                f"Invalid IndexKey format: '{s}'. Expected 'version:lang' (e.g., 'v1:zh')"
            )
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


class Settings(BaseSettings):
    """Application settings loaded from environment variables.

    This unified settings class contains all configuration options.
    CLI arguments take precedence over environment variables.
    """

    model_config = SettingsConfigDict(
        env_prefix="CANGJIE_",
        env_file=".env",
        env_file_encoding="utf-8",
        extra="ignore",
        populate_by_name=True,
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
    rerank_top_k: int = Field(
        default=5, description="Number of results to return after reranking"
    )
    rerank_initial_k: int = Field(
        default=20, description="Number of candidates to retrieve before reranking"
    )

    # Data directory (default: ~/.cangjie-mcp)
    data_dir: Path = Field(
        default_factory=_get_default_data_dir,
        description="Data directory path",
    )

    # Prebuilt index URL
    prebuilt_url: str | None = Field(default=None, description="Prebuilt index download URL")

    # OpenAI-compatible API settings
    # These settings work with OpenAI, SiliconFlow, and other compatible providers.
    # Set OPENAI_BASE_URL to use alternative providers (e.g., https://api.siliconflow.cn/v1)
    openai_api_key: str | None = Field(
        default=None,
        description="OpenAI-compatible API Key (works with SiliconFlow, etc.)",
        validation_alias=AliasChoices("openai_api_key", "OPENAI_API_KEY"),
    )
    openai_base_url: str = Field(
        default="https://api.openai.com/v1",
        description="OpenAI-compatible API Base URL",
        validation_alias=AliasChoices("openai_base_url", "OPENAI_BASE_URL"),
    )
    openai_model: str = Field(
        default="text-embedding-3-small",
        description="OpenAI-compatible embedding model",
        validation_alias=AliasChoices("openai_model", "OPENAI_MODEL"),
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
        description="Comma-separated list of indexes to load (e.g., 'v1:zh,latest:en')",
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
    """Get or create settings instance."""
    global _settings
    if _settings is None:
        _settings = Settings()
    return _settings


def reset_settings() -> None:
    """Reset the global settings instance (useful for testing)."""
    global _settings
    _settings = None


def update_settings(
    *,
    docs_version: str | None = None,
    docs_lang: str | None = None,
    embedding_type: str | None = None,
    local_model: str | None = None,
    rerank_type: str | None = None,
    rerank_model: str | None = None,
    rerank_top_k: int | None = None,
    rerank_initial_k: int | None = None,
    data_dir: Path | None = None,
    prebuilt_url: str | None = None,
    openai_api_key: str | None = None,
    openai_base_url: str | None = None,
    openai_model: str | None = None,
    http_host: str | None = None,
    http_port: int | None = None,
    indexes: str | None = None,
) -> Settings:
    """Update settings with new values (used by CLI to override env vars).

    Only non-None values will override existing settings.
    """
    global _settings

    # Collect non-None overrides
    overrides: dict[str, str | int | Path] = {}
    if docs_version is not None:
        overrides["docs_version"] = docs_version
    if docs_lang is not None:
        overrides["docs_lang"] = docs_lang
    if embedding_type is not None:
        overrides["embedding_type"] = embedding_type
    if local_model is not None:
        overrides["local_model"] = local_model
    if rerank_type is not None:
        overrides["rerank_type"] = rerank_type
    if rerank_model is not None:
        overrides["rerank_model"] = rerank_model
    if rerank_top_k is not None:
        overrides["rerank_top_k"] = rerank_top_k
    if rerank_initial_k is not None:
        overrides["rerank_initial_k"] = rerank_initial_k
    if data_dir is not None:
        overrides["data_dir"] = data_dir
    if prebuilt_url is not None:
        overrides["prebuilt_url"] = prebuilt_url
    if openai_api_key is not None:
        overrides["openai_api_key"] = openai_api_key
    if openai_base_url is not None:
        overrides["openai_base_url"] = openai_base_url
    if openai_model is not None:
        overrides["openai_model"] = openai_model
    if http_host is not None:
        overrides["http_host"] = http_host
    if http_port is not None:
        overrides["http_port"] = http_port
    if indexes is not None:
        overrides["indexes"] = indexes

    _settings = get_settings().model_copy(update=overrides)
    return _settings
