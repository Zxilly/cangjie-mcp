"""Integration tests for embedding providers.

These tests verify the complete workflow of document loading
and indexing using different embedding providers.
"""

import os
from pathlib import Path

import pytest

from cangjie_mcp.config import IndexInfo, Settings
from cangjie_mcp.indexer.embeddings import (
    LocalEmbedding,
    OpenAIEmbeddingProvider,
    get_embedding_provider,
    reset_embedding_provider,
)
from cangjie_mcp.indexer.loader import DocumentLoader
from cangjie_mcp.indexer.store import VectorStore
from tests.constants import CANGJIE_DOCS_VERSION


def _has_openai_credentials() -> bool:
    """Check if OpenAI credentials are available via environment variable."""
    api_key = os.environ.get("OPENAI_API_KEY", "")
    return bool(api_key and api_key != "your-openai-api-key-here")


class TestEmbeddingIntegration:
    """Integration tests using the configured embedding provider."""

    def test_load_and_index_documents(
        self,
        integration_docs_dir: Path,
        local_settings: Settings,
    ) -> None:
        """Test complete document loading and indexing workflow."""
        reset_embedding_provider()
        loader = DocumentLoader(integration_docs_dir)
        documents = loader.load_all_documents()

        assert len(documents) == 6
        assert all(doc.text for doc in documents)
        assert all(doc.metadata.get("category") for doc in documents)

        embedding_provider = get_embedding_provider(local_settings)
        assert isinstance(embedding_provider, LocalEmbedding | OpenAIEmbeddingProvider)

        store = VectorStore(
            db_path=IndexInfo.from_settings(local_settings).chroma_db_dir,
            embedding_provider=embedding_provider,
        )

        store.index_documents(documents)

        assert store.is_indexed()
        assert store.collection.count() > 0

    def test_semantic_search(self, local_indexed_store: VectorStore) -> None:
        """Test semantic search returns relevant results."""
        results = local_indexed_store.search(query="如何定义函数", top_k=3)

        assert len(results) > 0
        assert any("func" in r.text.lower() or "函数" in r.text for r in results)

    def test_search_with_category_filter(self, local_indexed_store: VectorStore) -> None:
        """Test search with category filtering."""
        results = local_indexed_store.search(
            query="编译器使用",
            category="tools",
            top_k=5,
        )

        assert len(results) > 0
        assert all(r.metadata.category == "tools" for r in results)

    def test_version_matching(
        self,
        local_indexed_store: VectorStore,
    ) -> None:
        """Test version matching functionality."""
        assert local_indexed_store.version_matches(CANGJIE_DOCS_VERSION, "zh")
        assert not local_indexed_store.version_matches(CANGJIE_DOCS_VERSION, "en")
        assert not local_indexed_store.version_matches("other", "zh")


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
            db_path=IndexInfo.from_settings(openai_settings).chroma_db_dir,
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
            db_path=IndexInfo.from_settings(openai_settings).chroma_db_dir,
            embedding_provider=embedding_provider,
        )

        loader = DocumentLoader(integration_docs_dir)
        documents = loader.load_all_documents()
        store.index_documents(documents)

        results = store.search(query="如何定义函数", top_k=3)

        assert len(results) > 0
        assert any("func" in r.text.lower() or "函数" in r.text for r in results)
