"""End-to-end integration tests simulating real usage.

These tests verify cross-cutting concerns not covered by individual
tool or store tests: metadata preservation during indexing and
multilingual search behavior.
"""

from pathlib import Path

from cangjie_mcp.indexer.loader import DocumentLoader
from cangjie_mcp.indexer.store import VectorStore


class TestEndToEndWorkflow:
    """End-to-end integration tests simulating real usage."""

    def test_indexing_preserves_document_structure(
        self,
        integration_docs_dir: Path,
        local_indexed_store: VectorStore,
    ) -> None:
        """Test that indexing preserves document metadata correctly."""
        loader = DocumentLoader(integration_docs_dir)
        documents = loader.load_all_documents()

        # Verify documents have correct metadata
        categories = {doc.metadata.get("category") for doc in documents}
        assert "basics" in categories
        assert "syntax" in categories
        assert "tools" in categories

        # Verify search results maintain metadata (use session-scoped store
        # to avoid redundant re-indexing)
        results = local_indexed_store.search(query="仓颉", top_k=10)
        for result in results:
            assert result.metadata.category in categories
            assert result.metadata.topic != ""

    def test_multilingual_search(
        self,
        local_indexed_store: VectorStore,
    ) -> None:
        """Test searching with Chinese and English queries."""
        # Chinese query
        zh_results = local_indexed_store.search(query="函数", top_k=3)
        assert len(zh_results) > 0

        # English-like query (code keywords)
        en_results = local_indexed_store.search(query="func main", top_k=3)
        assert len(en_results) > 0

        # Mixed query
        mixed_results = local_indexed_store.search(query="Hello 世界", top_k=3)
        assert len(mixed_results) > 0
