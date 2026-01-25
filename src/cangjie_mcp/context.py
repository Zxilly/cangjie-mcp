"""Application context management for Cangjie MCP.

This module provides a unified AppContext class that replaces scattered
global state and singleton patterns with a clean dependency injection approach.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from cangjie_mcp.config import Settings
    from cangjie_mcp.indexer.embeddings import EmbeddingProvider
    from cangjie_mcp.indexer.reranker import RerankerProvider
    from cangjie_mcp.indexer.store import VectorStore


@dataclass
class AppContext:
    """Centralized application context for dependency injection.

    This class replaces the scattered global state (_settings, SingletonProvider)
    with a unified context that can be passed explicitly or accessed globally.

    Usage:
        # Create context from settings
        ctx = AppContext.from_settings(settings)

        # Access components
        store = ctx.get_vector_store()
        embedder = ctx.get_embedding_provider()

        # Or use the global context
        set_context(ctx)
        ctx = get_context()
    """

    settings: Settings
    _embedding_provider: EmbeddingProvider | None = field(default=None, repr=False)
    _reranker_provider: RerankerProvider | None = field(default=None, repr=False)
    _vector_store: VectorStore | None = field(default=None, repr=False)

    @classmethod
    def from_settings(cls, settings: Settings) -> AppContext:
        """Create an AppContext from settings.

        This is a lazy initialization - providers are created on first access.

        Args:
            settings: Application settings

        Returns:
            New AppContext instance
        """
        return cls(settings=settings)

    def get_embedding_provider(self) -> EmbeddingProvider:
        """Get or create the embedding provider.

        Returns:
            EmbeddingProvider instance (cached)
        """
        if self._embedding_provider is None:
            from cangjie_mcp.indexer.embeddings import create_embedding_provider

            self._embedding_provider = create_embedding_provider(self.settings)
        return self._embedding_provider

    def get_reranker_provider(self) -> RerankerProvider | None:
        """Get or create the reranker provider.

        Returns:
            RerankerProvider instance or None if reranking is disabled
        """
        if self._reranker_provider is None and self.settings.rerank_type != "none":
            from cangjie_mcp.indexer.reranker import create_reranker_provider

            self._reranker_provider = create_reranker_provider(
                rerank_type=self.settings.rerank_type,
                local_model=self.settings.rerank_model,
                api_key=self.settings.openai_api_key,
                api_model=self.settings.rerank_model,
                api_base_url=self.settings.openai_base_url,
            )
        return self._reranker_provider

    def get_vector_store(self, with_rerank: bool = True) -> VectorStore:
        """Get or create the vector store.

        Args:
            with_rerank: Whether to include reranking capability

        Returns:
            VectorStore instance (cached)
        """
        if self._vector_store is None:
            from cangjie_mcp.indexer.store import VectorStore

            reranker = self.get_reranker_provider() if with_rerank else None
            self._vector_store = VectorStore(
                db_path=self.settings.chroma_db_dir,
                embedding_provider=self.get_embedding_provider(),
                reranker=reranker,
            )
        return self._vector_store

    def reset(self) -> None:
        """Reset all cached providers (useful for testing)."""
        self._embedding_provider = None
        self._reranker_provider = None
        self._vector_store = None


# Global context instance
_context: AppContext | None = None


def get_context() -> AppContext:
    """Get the global application context.

    Returns:
        The global AppContext instance

    Raises:
        RuntimeError: If context not initialized
    """
    if _context is None:
        raise RuntimeError("AppContext not initialized. Call set_context() first.")
    return _context


def set_context(ctx: AppContext) -> None:
    """Set the global application context.

    Args:
        ctx: AppContext instance to set as global
    """
    global _context
    _context = ctx


def reset_context() -> None:
    """Reset the global context (useful for testing)."""
    global _context
    if _context is not None:
        _context.reset()
    _context = None


def get_default_data_dir() -> Path:
    """Get the default data directory (~/.cangjie-mcp).

    Returns:
        Path to the default data directory
    """
    return Path.home() / ".cangjie-mcp"
