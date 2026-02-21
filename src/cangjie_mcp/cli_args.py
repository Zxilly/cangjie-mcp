"""Shared CLI argument definitions for cangjie-mcp.

This module provides a centralized definition of CLI arguments to eliminate
duplication across different commands (main, server).
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Annotated, Literal

import typer

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
)


@dataclass
class DocsArgs:
    """Shared documentation-related CLI arguments.

    This dataclass holds all the common arguments used across different
    CLI commands (main, server).
    """

    docs_version: str = DEFAULT_DOCS_VERSION
    lang: str = DEFAULT_DOCS_LANG
    embedding: str = DEFAULT_EMBEDDING_TYPE
    local_model: str = DEFAULT_LOCAL_MODEL
    openai_api_key: str | None = None
    openai_base_url: str = DEFAULT_OPENAI_BASE_URL
    openai_model: str = DEFAULT_OPENAI_MODEL
    rerank: str = DEFAULT_RERANK_TYPE
    rerank_model: str = DEFAULT_RERANK_MODEL
    rerank_top_k: int = DEFAULT_RERANK_TOP_K
    rerank_initial_k: int = DEFAULT_RERANK_INITIAL_K
    chunk_size: int = DEFAULT_CHUNK_MAX_SIZE
    data_dir: Path | None = None


# Type aliases for annotated CLI options
DocsVersionOption = Annotated[
    str,
    typer.Option(
        "--docs-version",
        "-V",
        help="Documentation version (git tag)",
        envvar="CANGJIE_DOCS_VERSION",
        show_default=True,
    ),
]

LangOption = Annotated[
    str,
    typer.Option(
        "--lang",
        "-l",
        help="Documentation language (zh/en)",
        envvar="CANGJIE_DOCS_LANG",
        show_default=True,
    ),
]

EmbeddingOption = Annotated[
    str,
    typer.Option(
        "--embedding",
        "-e",
        help="Embedding type: none (BM25 only), local, or openai",
        envvar="CANGJIE_EMBEDDING_TYPE",
        show_default=True,
    ),
]

LocalModelOption = Annotated[
    str,
    typer.Option(
        "--local-model",
        help="Local HuggingFace embedding model name",
        envvar="CANGJIE_LOCAL_MODEL",
        show_default=True,
    ),
]

OpenAIApiKeyOption = Annotated[
    str | None,
    typer.Option(
        "--openai-api-key",
        help="OpenAI API key",
        envvar="OPENAI_API_KEY",
    ),
]

OpenAIBaseUrlOption = Annotated[
    str,
    typer.Option(
        "--openai-base-url",
        help="OpenAI API base URL",
        envvar="OPENAI_BASE_URL",
        show_default=True,
    ),
]

OpenAIModelOption = Annotated[
    str,
    typer.Option(
        "--openai-model",
        help="OpenAI embedding model",
        envvar="OPENAI_EMBEDDING_MODEL",
        show_default=True,
    ),
]

RerankOption = Annotated[
    str,
    typer.Option(
        "--rerank",
        "-r",
        help="Rerank type (none/local/openai)",
        envvar="CANGJIE_RERANK_TYPE",
        show_default=True,
    ),
]

RerankModelOption = Annotated[
    str,
    typer.Option(
        "--rerank-model",
        help="Rerank model name",
        envvar="CANGJIE_RERANK_MODEL",
        show_default=True,
    ),
]

RerankTopKOption = Annotated[
    int,
    typer.Option(
        "--rerank-top-k",
        help="Number of results after reranking",
        envvar="CANGJIE_RERANK_TOP_K",
        show_default=True,
    ),
]

RerankInitialKOption = Annotated[
    int,
    typer.Option(
        "--rerank-initial-k",
        help="Number of candidates before reranking",
        envvar="CANGJIE_RERANK_INITIAL_K",
        show_default=True,
    ),
]

ChunkSizeOption = Annotated[
    int,
    typer.Option(
        "--chunk-size",
        help="Max chunk size in characters",
        envvar="CANGJIE_CHUNK_MAX_SIZE",
        show_default=True,
    ),
]

RRFKOption = Annotated[
    int,
    typer.Option(
        "--rrf-k",
        help="RRF constant k for hybrid search fusion",
        envvar="CANGJIE_RRF_K",
        show_default=True,
    ),
]

DataDirOption = Annotated[
    Path | None,
    typer.Option(
        "--data-dir",
        "-d",
        help="Data directory path",
        envvar="CANGJIE_DATA_DIR",
        show_default="~/.cangjie-mcp",
    ),
]

LogFileOption = Annotated[
    Path | None,
    typer.Option(
        "--log-file",
        help="Log file path for application logging",
        envvar="CANGJIE_LOG_FILE",
    ),
]

DebugOption = Annotated[
    bool,
    typer.Option(
        "--debug/--no-debug",
        help="Enable debug mode (log stdio traffic to log file)",
        envvar="CANGJIE_DEBUG",
    ),
]

ServerUrlOption = Annotated[
    str | None,
    typer.Option(
        "--server-url",
        help="URL of a remote cangjie-mcp server to forward queries to",
        envvar="CANGJIE_SERVER_URL",
    ),
]

HostOption = Annotated[
    str,
    typer.Option(
        "--host",
        help="Host to bind the HTTP server to",
        envvar="CANGJIE_SERVER_HOST",
        show_default=True,
    ),
]

PortOption = Annotated[
    int,
    typer.Option(
        "--port",
        "-p",
        help="Port to bind the HTTP server to",
        envvar="CANGJIE_SERVER_PORT",
        show_default=True,
    ),
]


def validate_lang(value: str) -> Literal["zh", "en"]:
    """Validate language value."""
    if value == "zh" or value == "en":
        return value
    raise typer.BadParameter(f"Invalid language: {value}. Must be one of: zh, en.")


def validate_embedding_type(value: str) -> Literal["none", "local", "openai"]:
    """Validate embedding type value."""
    if value == "none" or value == "local" or value == "openai":
        return value
    raise typer.BadParameter(f"Invalid embedding type: {value}. Must be one of: none, local, openai.")


def validate_rerank_type(value: str) -> Literal["none", "local", "openai"]:
    """Validate rerank type value."""
    if value == "none" or value == "local" or value == "openai":
        return value
    raise typer.BadParameter(f"Invalid rerank type: {value}. Must be one of: none, local, openai.")


def validate_docs_args(
    args: DocsArgs,
) -> tuple[
    Literal["zh", "en"],
    Literal["none", "local", "openai"],
    Literal["none", "local", "openai"],
]:
    """Validate and convert DocsArgs to proper literal types.

    Args:
        args: DocsArgs instance with string values

    Returns:
        Tuple of (validated_lang, validated_embedding, validated_rerank)

    Raises:
        typer.BadParameter: If any value is invalid
    """
    return (
        validate_lang(args.lang),
        validate_embedding_type(args.embedding),
        validate_rerank_type(args.rerank),
    )
