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
- CANGJIE_RERANK_TYPE -> rerank_type
- CANGJIE_RERANK_LOCAL_MODEL -> rerank_local_model
- CANGJIE_RERANK_TOP_K -> rerank_top_k
- CANGJIE_RERANK_INITIAL_K -> rerank_initial_k
- CANGJIE_DATA_DIR -> data_dir
- CANGJIE_PREBUILT_URL -> prebuilt_url
- OPENAI_API_KEY -> openai_api_key
- OPENAI_BASE_URL -> openai_base_url
- OPENAI_MODEL -> openai_model
- SILICONFLOW_API_KEY -> siliconflow_api_key
- SILICONFLOW_BASE_URL -> siliconflow_base_url
- SILICONFLOW_RERANK_MODEL -> siliconflow_rerank_model
"""

from pathlib import Path
from typing import Literal

from pydantic import AliasChoices, Field
from pydantic_settings import BaseSettings, SettingsConfigDict


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
    rerank_type: Literal["none", "local", "siliconflow"] = Field(
        default="none", description="Reranker type (none to disable)"
    )
    rerank_local_model: str = Field(
        default="BAAI/bge-reranker-v2-m3",
        description="Local cross-encoder model for reranking",
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

    # OpenAI settings (accepts both field name and OPENAI_ env var)
    openai_api_key: str | None = Field(
        default=None,
        description="OpenAI API Key",
        validation_alias=AliasChoices("openai_api_key", "OPENAI_API_KEY"),
    )
    openai_base_url: str = Field(
        default="https://api.openai.com/v1",
        description="OpenAI API Base URL",
        validation_alias=AliasChoices("openai_base_url", "OPENAI_BASE_URL"),
    )
    openai_model: str = Field(
        default="text-embedding-3-small",
        description="OpenAI embedding model",
        validation_alias=AliasChoices("openai_model", "OPENAI_MODEL"),
    )

    # SiliconFlow settings (for rerank API)
    siliconflow_api_key: str | None = Field(
        default=None,
        description="SiliconFlow API Key for reranking",
        validation_alias=AliasChoices("siliconflow_api_key", "SILICONFLOW_API_KEY"),
    )
    siliconflow_base_url: str = Field(
        default="https://api.siliconflow.cn/v1",
        description="SiliconFlow API Base URL",
        validation_alias=AliasChoices("siliconflow_base_url", "SILICONFLOW_BASE_URL"),
    )
    siliconflow_rerank_model: str = Field(
        default="BAAI/bge-reranker-v2-m3",
        description="SiliconFlow rerank model",
        validation_alias=AliasChoices("siliconflow_rerank_model", "SILICONFLOW_RERANK_MODEL"),
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


# Legacy compatibility aliases
OpenAISettings = Settings
RerankSettings = Settings


# Global settings instance
_settings: Settings | None = None


def get_settings() -> Settings:
    """Get or create settings instance."""
    global _settings
    if _settings is None:
        _settings = Settings()
    return _settings


def get_openai_settings() -> Settings:
    """Get settings instance (legacy compatibility).

    Deprecated: Use get_settings() instead.
    """
    return get_settings()


def get_rerank_settings() -> Settings:
    """Get settings instance (legacy compatibility).

    Deprecated: Use get_settings() instead.
    """
    return get_settings()


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
    rerank_local_model: str | None = None,
    rerank_top_k: int | None = None,
    rerank_initial_k: int | None = None,
    data_dir: Path | None = None,
    prebuilt_url: str | None = None,
    openai_api_key: str | None = None,
    openai_base_url: str | None = None,
    openai_model: str | None = None,
    siliconflow_api_key: str | None = None,
    siliconflow_base_url: str | None = None,
    siliconflow_rerank_model: str | None = None,
) -> Settings:
    """Update settings with new values (used by CLI to override env vars).

    Only non-None values will override existing settings.
    """
    global _settings
    current = get_settings()
    new_values = current.model_dump()

    # Update only provided values
    overrides = {
        "docs_version": docs_version,
        "docs_lang": docs_lang,
        "embedding_type": embedding_type,
        "local_model": local_model,
        "rerank_type": rerank_type,
        "rerank_local_model": rerank_local_model,
        "rerank_top_k": rerank_top_k,
        "rerank_initial_k": rerank_initial_k,
        "data_dir": data_dir,
        "prebuilt_url": prebuilt_url,
        "openai_api_key": openai_api_key,
        "openai_base_url": openai_base_url,
        "openai_model": openai_model,
        "siliconflow_api_key": siliconflow_api_key,
        "siliconflow_base_url": siliconflow_base_url,
        "siliconflow_rerank_model": siliconflow_rerank_model,
    }

    for key, value in overrides.items():
        if value is not None:
            new_values[key] = value

    _settings = Settings(**new_values)
    return _settings
