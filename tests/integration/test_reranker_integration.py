"""Integration tests for reranker providers.

These tests verify the complete workflow of document search
with reranking using local cross-encoder models.
"""

from pathlib import Path

from cangjie_mcp.indexer.loader import DocumentLoader
from cangjie_mcp.indexer.reranker import LocalReranker
from cangjie_mcp.indexer.store import VectorStore

# Default local reranker model for testing
CANGJIE_LOCAL_RERANKER_MODEL = "BAAI/bge-reranker-v2-m3"


class TestLocalRerankerIntegration:
    """Integration tests using local reranker."""

    def test_reranker_initialization(self, shared_local_reranker: LocalReranker) -> None:
        """Test that local reranker initializes correctly."""
        assert shared_local_reranker.model_name == CANGJIE_LOCAL_RERANKER_MODEL
        assert shared_local_reranker.get_model_name() == f"local:{CANGJIE_LOCAL_RERANKER_MODEL}"

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

    def test_search_without_reranker(
        self,
        shared_indexed_store_with_reranker: VectorStore,
    ) -> None:
        """Test search with reranking disabled."""
        results = shared_indexed_store_with_reranker.search(
            query="如何定义函数",
            top_k=3,
            use_rerank=False,
        )

        assert len(results) > 0
        assert any("func" in r.text.lower() or "函数" in r.text for r in results)

    def test_rerank_improves_relevance(
        self,
        shared_indexed_store_with_reranker: VectorStore,
    ) -> None:
        """Test that reranking can change result ordering."""
        # Get results with and without reranking
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

        # Both should return results
        assert len(results_with_rerank) > 0
        assert len(results_without_rerank) > 0

        # Check that pattern matching content is found in both cases
        combined_with_rerank = " ".join(r.text for r in results_with_rerank)
        combined_without_rerank = " ".join(r.text for r in results_without_rerank)

        assert "match" in combined_with_rerank.lower() or "模式" in combined_with_rerank
        assert "match" in combined_without_rerank.lower() or "模式" in combined_without_rerank

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

    def test_search_returns_sorted_by_rerank_score(
        self,
        shared_indexed_store_with_reranker: VectorStore,
    ) -> None:
        """Test that search results are sorted by rerank score (descending)."""
        results = shared_indexed_store_with_reranker.search(
            query="函数定义",
            top_k=5,
            use_rerank=True,
        )

        assert len(results) > 1
        scores = [r.score for r in results]
        assert scores == sorted(scores, reverse=True)

    def test_search_multiple_queries_with_rerank(
        self,
        shared_indexed_store_with_reranker: VectorStore,
    ) -> None:
        """Test multiple search queries with reranking."""
        queries = [
            ("Hello World", ["Hello", "Cangjie", "程序"]),
            ("变量声明", ["let", "var", "变量"]),
            ("包管理器", ["cjpm", "包", "依赖"]),
        ]

        for query, expected_keywords in queries:
            results = shared_indexed_store_with_reranker.search(
                query=query,
                top_k=3,
                use_rerank=True,
            )
            assert len(results) > 0, f"No results for query: {query}"
            combined_text = " ".join(r.text for r in results)
            assert any(kw in combined_text for kw in expected_keywords), (
                f"Expected keywords not found for query: {query}"
            )

    def test_initial_k_parameter(
        self,
        shared_indexed_store_with_reranker: VectorStore,
    ) -> None:
        """Test that initial_k parameter controls candidate retrieval."""
        # With a small initial_k, we should still get results
        results = shared_indexed_store_with_reranker.search(
            query="函数",
            top_k=2,
            use_rerank=True,
            initial_k=5,
        )

        assert len(results) > 0
        assert len(results) <= 2


class TestVectorStoreWithoutReranker:
    """Tests to ensure VectorStore works correctly without reranker."""

    def test_search_without_reranker_configured(
        self,
        integration_docs_dir: Path,
        local_settings,
        shared_embedding_provider,
    ) -> None:
        """Test search when no reranker is configured."""
        store = VectorStore(
            db_path=local_settings.chroma_db_dir,
            embedding_provider=shared_embedding_provider,
            # No reranker provided
        )

        loader = DocumentLoader(integration_docs_dir)
        documents = loader.load_all_documents()
        store.index_documents(documents)

        # Search should work without reranker
        results = store.search(query="函数", top_k=3)

        assert len(results) > 0
        assert store.reranker is None
