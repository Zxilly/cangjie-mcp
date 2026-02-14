"""Settings factory for creating Settings from CLI arguments.

This module provides utility functions for creating Settings objects
from validated CLI arguments, reducing duplication in CLI commands.
"""

from __future__ import annotations

from pathlib import Path
from typing import Literal

from cangjie_mcp.config import Settings


def _get_default_data_dir() -> Path:
    """Get the default data directory (~/.cangjie-mcp)."""
    return Path.home() / ".cangjie-mcp"


def create_settings_from_args(
    docs_version: str,
    docs_lang: Literal["zh", "en"],
    embedding_type: Literal["local", "openai"],
    local_model: str,
    openai_api_key: str | None,
    openai_base_url: str,
    openai_model: str,
    rerank_type: Literal["none", "local", "openai"],
    rerank_model: str,
    rerank_top_k: int,
    rerank_initial_k: int,
    chunk_max_size: int,
    data_dir: Path | None,
    prebuilt_url: str | None = None,
) -> Settings:
    """Create a Settings object from validated CLI arguments.

    This factory function encapsulates the common pattern of creating
    Settings from CLI arguments, reducing duplication across commands.

    Args:
        docs_version: Documentation version (git tag)
        docs_lang: Documentation language
        embedding_type: Type of embedding (local/openai)
        local_model: HuggingFace model name for local embedding
        openai_api_key: OpenAI API key
        openai_base_url: OpenAI API base URL
        openai_model: OpenAI embedding model
        rerank_type: Type of reranking (none/local/openai)
        rerank_model: Reranker model name
        rerank_top_k: Number of results after reranking
        rerank_initial_k: Number of candidates before reranking
        chunk_max_size: Maximum chunk size in characters
        data_dir: Data directory path
        prebuilt_url: URL to download prebuilt index from

    Returns:
        Configured Settings instance
    """
    return Settings(
        docs_version=docs_version,
        docs_lang=docs_lang,
        embedding_type=embedding_type,
        local_model=local_model,
        openai_api_key=openai_api_key,
        openai_base_url=openai_base_url,
        openai_model=openai_model,
        rerank_type=rerank_type,
        rerank_model=rerank_model,
        rerank_top_k=rerank_top_k,
        rerank_initial_k=rerank_initial_k,
        chunk_max_size=chunk_max_size,
        data_dir=data_dir if data_dir else _get_default_data_dir(),
        prebuilt_url=prebuilt_url,
    )
