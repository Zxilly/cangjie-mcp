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
    from cangjie_mcp.indexer.bm25_store import BM25Store
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
    """Search index backed by BM25 and optionally a ChromaDB vector store.

    When only BM25 is configured (embedding_type="none"), queries use
    pure BM25 keyword search.  When an embedding model is also configured,
    BM25 and vector search run in parallel and results are fused via RRF.
    """

    def __init__(self, settings: Settings) -> None:
        self._settings = settings
        self._store: VectorStore | None = None
        self._bm25_store: BM25Store | None = None

    def init(self) -> IndexInfo:
        """Initialize the local index (clone repo, build if needed, load stores)."""
        from cangjie_mcp.indexer.bm25_store import BM25Store
        from cangjie_mcp.indexer.initializer import initialize_and_index
        from cangjie_mcp.indexer.store import create_vector_store

        index_info = initialize_and_index(self._settings)

        # Always load BM25 store
        bm25 = BM25Store(index_info.bm25_index_dir)
        if bm25.load():
            self._bm25_store = bm25
        else:
            logger.warning("Failed to load BM25 index from %s", index_info.bm25_index_dir)

        # Load vector store only when embedding is configured
        if self._settings.has_embedding:
            self._store = create_vector_store(index_info, self._settings)

        return index_info

    async def query(
        self,
        query: str,
        top_k: int = 5,
        category: str | None = None,
        rerank: bool = True,
    ) -> list[SearchResult]:
        """Search using BM25 and optionally vector search with RRF fusion."""
        if self._bm25_store is None and self._store is None:
            return []

        # Pure BM25 mode
        if self._store is None:
            if self._bm25_store is None:
                return []
            return await asyncio.to_thread(
                self._bm25_store.search,
                query=query,
                top_k=top_k,
                category=category,
            )

        # Hybrid mode: run BM25 and vector search in parallel, then fuse
        from cangjie_mcp.indexer.fusion import reciprocal_rank_fusion

        store = self._store
        # Retrieve more candidates for fusion
        fusion_k = max(top_k * 3, 20)

        async def _bm25_search() -> list[SearchResult]:
            if self._bm25_store is None:
                return []
            return await asyncio.to_thread(
                self._bm25_store.search,
                query=query,
                top_k=fusion_k,
                category=category,
            )

        async def _vector_search() -> list[SearchResult]:
            return await asyncio.to_thread(
                store.search,
                query=query,
                top_k=fusion_k,
                category=category,
                use_rerank=False,  # rerank after fusion
            )

        bm25_results, vector_results = await asyncio.gather(
            _bm25_search(),
            _vector_search(),
        )

        reranker = store.reranker
        fused = reciprocal_rank_fusion(
            [bm25_results, vector_results],
            k=self._settings.rrf_k,
            top_k=top_k if not (rerank and reranker) else max(top_k * 4, 20),
        )

        # Apply reranking on fused results if enabled
        if rerank and reranker is not None:
            from llama_index.core.schema import NodeWithScore, TextNode

            nodes = [
                NodeWithScore(
                    node=TextNode(text=r.text, metadata=r.metadata.model_dump()),
                    score=r.score,
                )
                for r in fused
            ]
            reranked = reranker.rerank(query=query, nodes=nodes, top_k=top_k)

            from cangjie_mcp.indexer.store import SearchResult, SearchResultMetadata

            return [
                SearchResult(
                    text=n.text,
                    score=n.score if n.score is not None else 0.0,
                    metadata=SearchResultMetadata.from_node_metadata(n.metadata),
                )
                for n in reranked[:top_k]
            ]

        return fused[:top_k]


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
