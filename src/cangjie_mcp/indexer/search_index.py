"""SearchIndex abstraction for local and remote index access.

Provides a unified async interface for querying documentation indexes,
whether backed by a local ChromaDB store or a remote HTTP server.
"""

from __future__ import annotations

import asyncio
from abc import ABC, abstractmethod
from typing import TYPE_CHECKING

from cangjie_mcp.utils import logger

if TYPE_CHECKING:
    from cangjie_mcp.config import IndexInfo, Settings
    from cangjie_mcp.indexer.store import SearchResult, VectorStore


class SearchIndex(ABC):
    """Abstract base class for search index implementations."""

    @abstractmethod
    def init(self) -> IndexInfo:
        """Initialize the index and return metadata.

        Returns:
            IndexInfo describing the active index.
        """
        ...

    @abstractmethod
    async def query(
        self,
        query: str,
        top_k: int = 5,
        category: str | None = None,
        rerank: bool = True,
    ) -> list[SearchResult]:
        """Search the index.

        Args:
            query: Search query string.
            top_k: Number of results to return.
            category: Optional category filter.
            rerank: Whether to apply reranking (if available).

        Returns:
            List of search results.
        """
        ...


class LocalSearchIndex(SearchIndex):
    """Search index backed by a local ChromaDB store.

    Wraps the existing initialize_and_index + VectorStore code behind
    the SearchIndex interface. Queries are dispatched to a thread to
    avoid blocking the event loop.
    """

    def __init__(self, settings: Settings) -> None:
        self._settings = settings
        self._store: VectorStore | None = None

    def init(self) -> IndexInfo:
        """Initialize the local index (clone repo, build if needed, load store)."""
        from cangjie_mcp.indexer.initializer import initialize_and_index
        from cangjie_mcp.indexer.store import create_vector_store

        index_info = initialize_and_index(self._settings)
        self._store = create_vector_store(index_info, self._settings)
        return index_info

    async def query(
        self,
        query: str,
        top_k: int = 5,
        category: str | None = None,
        rerank: bool = True,
    ) -> list[SearchResult]:
        """Search the local ChromaDB store in a background thread."""
        if self._store is None:
            return []

        return await asyncio.to_thread(
            self._store.search,
            query=query,
            top_k=top_k,
            category=category,
            use_rerank=rerank,
        )


class RemoteSearchIndex(SearchIndex):
    """Search index that delegates to a remote HTTP server.

    No embedding model or ChromaDB needed locally — all queries are
    forwarded to the server via httpx.
    """

    def __init__(self, server_url: str) -> None:
        self._server_url = server_url.rstrip("/")

    def init(self) -> IndexInfo:
        """Verify server reachability and retrieve IndexInfo."""
        import httpx

        from cangjie_mcp.config import IndexInfo

        url = f"{self._server_url}/info"
        logger.info("Connecting to remote server: %s", self._server_url)
        with httpx.Client(timeout=30.0) as client:
            resp = client.get(url)
            resp.raise_for_status()
            data = resp.json()

        return IndexInfo(
            version=data["version"],
            lang=data["lang"],
            embedding_model_name=data["embedding_model"],
            data_dir=__import__("pathlib").Path.home() / ".cangjie-mcp",
        )

    async def query(
        self,
        query: str,
        top_k: int = 5,
        category: str | None = None,
        rerank: bool = True,
    ) -> list[SearchResult]:
        """Forward query to the remote server."""
        import httpx

        from cangjie_mcp.indexer.store import SearchResult, SearchResultMetadata

        url = f"{self._server_url}/search"
        payload: dict[str, object] = {"query": query, "top_k": top_k, "rerank": rerank}
        if category:
            payload["category"] = category

        async with httpx.AsyncClient(timeout=60.0) as client:
            resp = await client.post(url, json=payload)
            resp.raise_for_status()
            data = resp.json()

        results: list[SearchResult] = []
        for item in data.get("results", []):
            meta = item.get("metadata", {})
            results.append(
                SearchResult(
                    text=item["text"],
                    score=item.get("score", 0.0),
                    metadata=SearchResultMetadata(
                        file_path=meta.get("file_path", ""),
                        category=meta.get("category", ""),
                        topic=meta.get("topic", ""),
                        title=meta.get("title", ""),
                        has_code=bool(meta.get("has_code", False)),
                    ),
                )
            )

        return results
