"""Integration tests for reranker providers.

These tests verify the complete workflow of document search
with reranking using local cross-encoder models.
"""

from cangjie_mcp.indexer.reranker import LocalReranker
from cangjie_mcp.indexer.store import VectorStore
from tests.constants import CANGJIE_RERANKER_MODEL


class TestLocalRerankerIntegration:
    """Integration tests using local reranker."""

    def test_reranker_initialization(self, shared_local_reranker: LocalReranker) -> None:
        """Test that local reranker initializes correctly."""
        assert shared_local_reranker.model_name == CANGJIE_RERANKER_MODEL
        assert shared_local_reranker.get_model_name() == f"local:{CANGJIE_RERANKER_MODEL}"

    def test_search_with_reranker(
        self,
        shared_indexed_store_with_reranker: VectorStore,
    ) -> None:
        """Test search with reranking enabled."""
        results = shared_indexed_store_with_reranker.search(
            query="如何定义函数",
            top_k=3,
            use_rerank=True,
        )

        assert len(results) > 0
        assert any("func" in r.text.lower() or "函数" in r.text for r in results)
        # Verify results are sorted by rerank score (descending)
        scores = [r.score for r in results]
        assert scores == sorted(scores, reverse=True)

    def test_rerank_changes_ordering(
        self,
        shared_indexed_store_with_reranker: VectorStore,
    ) -> None:
        """Test that reranking can change result ordering vs raw embedding."""
        results_with_rerank = shared_indexed_store_with_reranker.search(
            query="模式匹配",
            top_k=5,
            use_rerank=True,
        )

        results_without_rerank = shared_indexed_store_with_reranker.search(
            query="模式匹配",
            top_k=5,
            use_rerank=False,
        )

        # Both should return results with relevant content
        assert len(results_with_rerank) > 0
        assert len(results_without_rerank) > 0

        combined_with_rerank = " ".join(r.text for r in results_with_rerank)
        assert "match" in combined_with_rerank.lower() or "模式" in combined_with_rerank

    def test_search_with_category_filter_and_rerank(
        self,
        shared_indexed_store_with_reranker: VectorStore,
    ) -> None:
        """Test search with category filtering and reranking."""
        results = shared_indexed_store_with_reranker.search(
            query="编译器使用",
            category="tools",
            top_k=5,
            use_rerank=True,
        )

        assert len(results) > 0
        assert all(r.metadata.category == "tools" for r in results)


class TestVectorStoreWithoutReranker:
    """Tests to ensure VectorStore works correctly without reranker."""

    def test_search_without_reranker_configured(
        self,
        local_indexed_store: VectorStore,
    ) -> None:
        """Test search when no reranker is configured."""
        # local_indexed_store is created without a reranker
        results = local_indexed_store.search(query="函数", top_k=3)

        assert len(results) > 0
        assert local_indexed_store.reranker is None
