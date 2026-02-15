"""Centralized default values for cangjie-mcp.

This module serves as the single source of truth for all default values
used throughout the application. CLI options, configuration, and other
modules should import from here to avoid duplication.
"""

from pathlib import Path
from typing import Literal

# Documentation settings
DEFAULT_DOCS_VERSION: str = "latest"
DEFAULT_DOCS_LANG: Literal["zh", "en"] = "zh"

# Embedding settings
DEFAULT_EMBEDDING_TYPE: Literal["local", "openai"] = "local"
DEFAULT_LOCAL_MODEL: str = "paraphrase-multilingual-MiniLM-L12-v2"

# Rerank settings
DEFAULT_RERANK_TYPE: Literal["none", "local", "openai"] = "none"
DEFAULT_RERANK_MODEL: str = "BAAI/bge-reranker-v2-m3"
DEFAULT_RERANK_TOP_K: int = 5
DEFAULT_RERANK_INITIAL_K: int = 20

# Chunking settings
DEFAULT_CHUNK_MAX_SIZE: int = 6000

# OpenAI-compatible API settings
DEFAULT_OPENAI_BASE_URL: str = "https://api.openai.com/v1"
DEFAULT_OPENAI_MODEL: str = "text-embedding-3-small"

# Data directory
DEFAULT_DATA_DIR_NAME: str = ".cangjie-mcp"


def get_default_data_dir() -> Path:
    """Get the default data directory (~/.cangjie-mcp)."""
    return Path.home() / DEFAULT_DATA_DIR_NAME
