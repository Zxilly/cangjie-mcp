"""Integration tests for embedding providers.

These tests verify the complete workflow of document loading
and indexing using different embedding providers.
"""

from pathlib import Path

import pytest

from cangjie_mcp.config import OpenAISettings, Settings
from cangjie_mcp.indexer.embeddings import (
    LocalEmbedding,
    OpenAIEmbeddingProvider,
    get_embedding_provider,
    reset_embedding_provider,
)
from cangjie_mcp.indexer.loader import DocumentLoader
from cangjie_mcp.indexer.store import VectorStore

from tests.constants import CANGJIE_DOCS_VERSION, CANGJIE_LOCAL_MODEL


def _has_openai_credentials() -> bool:
    """Check if OpenAI credentials are available."""
    settings = OpenAISettings()
    return bool(settings.api_key and settings.api_key != "your-openai-api-key-here")


class TestLocalEmbeddingIntegration:
    """Integration tests using local embeddings."""

    def test_load_and_index_documents(
        self,
        integration_docs_dir: Path,
        local_settings: Settings,
    ) -> None:
        """Test complete document loading and indexing workflow with local embeddings."""
        reset_embedding_provider()
        loader = DocumentLoader(integration_docs_dir)
        documents = loader.load_all_documents()

        assert len(documents) == 6
        assert all(doc.text for doc in documents)
        assert all(doc.metadata.get("category") for doc in documents)

        embedding_provider = get_embedding_provider(local_settings)
        assert isinstance(embedding_provider, LocalEmbedding)
        assert embedding_provider.model_name == CANGJIE_LOCAL_MODEL

        store = VectorStore(
            db_path=local_settings.chroma_db_dir,
            embedding_provider=embedding_provider,
        )

        store.index_documents(documents)

        assert store.is_indexed()
        assert store.collection.count() > 0

    def test_semantic_search_with_local_embedding(
        self, local_indexed_store: VectorStore
    ) -> None:
        """Test semantic search with local embeddings."""
        results = local_indexed_store.search(query="如何定义函数", top_k=3)

        assert len(results) > 0
        assert any("func" in r.text.lower() or "函数" in r.text for r in results)

    def test_search_with_category_filter_local(
        self, local_indexed_store: VectorStore
    ) -> None:
        """Test search with category filtering using local embeddings."""
        results = local_indexed_store.search(
            query="编译器使用",
            category="tools",
            top_k=5,
        )

        assert len(results) > 0
        assert all(r.metadata.category == "tools" for r in results)

    def test_version_matching_local(
        self,
        local_indexed_store: VectorStore,
    ) -> None:
        """Test version matching functionality with local embeddings."""
        assert local_indexed_store.version_matches(CANGJIE_DOCS_VERSION, "zh")
        assert not local_indexed_store.version_matches(CANGJIE_DOCS_VERSION, "en")
        assert not local_indexed_store.version_matches("other", "zh")

    def test_search_multiple_queries(self, local_indexed_store: VectorStore) -> None:
        """Test multiple search queries with local embeddings."""
        queries = [
            ("Hello World", ["Hello", "Cangjie", "程序"]),
            ("变量声明", ["let", "var", "变量"]),
            ("模式匹配", ["match", "模式"]),
        ]

        for query, expected_keywords in queries:
            results = local_indexed_store.search(query=query, top_k=3)
            assert len(results) > 0, f"No results for query: {query}"
            combined_text = " ".join(r.text for r in results)
            assert any(
                kw in combined_text for kw in expected_keywords
            ), f"Expected keywords not found for query: {query}"

    def test_search_returns_sorted_by_score(
        self, local_indexed_store: VectorStore
    ) -> None:
        """Test that search results are sorted by score (descending)."""
        results = local_indexed_store.search(query="函数定义", top_k=5)

        assert len(results) > 1
        scores = [r.score for r in results]
        assert scores == sorted(scores, reverse=True)


@pytest.mark.skipif(
    not _has_openai_credentials(),
    reason="OpenAI credentials not configured",
)
class TestOpenAIEmbeddingIntegration:
    """Integration tests using OpenAI embeddings."""

    def test_load_and_index_documents_openai(
        self,
        integration_docs_dir: Path,
        openai_settings: Settings,
    ) -> None:
        """Test complete document loading and indexing workflow with OpenAI embeddings."""
        reset_embedding_provider()
        loader = DocumentLoader(integration_docs_dir)
        documents = loader.load_all_documents()

        assert len(documents) == 6

        embedding_provider = get_embedding_provider(openai_settings)
        assert isinstance(embedding_provider, OpenAIEmbeddingProvider)

        store = VectorStore(
            db_path=openai_settings.chroma_db_dir,
            embedding_provider=embedding_provider,
        )

        store.index_documents(documents)

        assert store.is_indexed()
        assert store.collection.count() > 0

    def test_semantic_search_with_openai_embedding(
        self,
        integration_docs_dir: Path,
        openai_settings: Settings,
    ) -> None:
        """Test semantic search with OpenAI embeddings."""
        reset_embedding_provider()
        embedding_provider = get_embedding_provider(openai_settings)
        store = VectorStore(
            db_path=openai_settings.chroma_db_dir,
            embedding_provider=embedding_provider,
        )

        loader = DocumentLoader(integration_docs_dir)
        documents = loader.load_all_documents()
        store.index_documents(documents)

        results = store.search(query="如何定义函数", top_k=3)

        assert len(results) > 0
        assert any("func" in r.text.lower() or "函数" in r.text for r in results)
