"""Integration tests for VectorStore functionality.

These tests cover additional VectorStore operations like
get_index, clear, and index_nodes.
"""

from pathlib import Path

from cangjie_mcp.config import Settings
from cangjie_mcp.indexer.chunker import create_chunker
from cangjie_mcp.indexer.embeddings import get_embedding_provider, reset_embedding_provider
from cangjie_mcp.indexer.loader import DocumentLoader
from cangjie_mcp.indexer.store import VectorStore
from tests.constants import CANGJIE_LOCAL_MODEL


class TestVectorStoreAdvanced:
    """Advanced integration tests for VectorStore."""

    def test_get_index_returns_existing_index(
        self,
        integration_docs_dir: Path,
        local_settings: Settings,
    ) -> None:
        """Test get_index returns existing index after indexing."""
        reset_embedding_provider()
        embedding_provider = get_embedding_provider(local_settings)
        store = VectorStore(
            db_path=local_settings.chroma_db_dir,
            embedding_provider=embedding_provider,
        )

        loader = DocumentLoader(integration_docs_dir)
        documents = loader.load_all_documents()
        store.index_documents(documents)

        # Get index should return the cached index
        index1 = store.get_index()
        index2 = store.get_index()

        assert index1 is not None
        assert index2 is not None
        assert index1 is index2  # Should be same cached instance

    def test_get_index_returns_none_when_empty(
        self,
        temp_data_dir: Path,
    ) -> None:
        """Test get_index returns None when store is empty."""
        reset_embedding_provider()
        settings = Settings(
            docs_version="test",
            docs_lang="zh",
            embedding_type="local",
            local_model=CANGJIE_LOCAL_MODEL,
            data_dir=temp_data_dir,
        )
        embedding_provider = get_embedding_provider(settings)
        store = VectorStore(
            db_path=settings.chroma_db_dir,
            embedding_provider=embedding_provider,
        )

        # Should return None when not indexed
        index = store.get_index()
        assert index is None

    def test_clear_index(
        self,
        integration_docs_dir: Path,
        local_settings: Settings,
    ) -> None:
        """Test clearing the index."""
        reset_embedding_provider()
        embedding_provider = get_embedding_provider(local_settings)
        store = VectorStore(
            db_path=local_settings.chroma_db_dir,
            embedding_provider=embedding_provider,
        )

        loader = DocumentLoader(integration_docs_dir)
        documents = loader.load_all_documents()
        store.index_documents(documents)
        store.save_metadata(
            version="test",
            lang="zh",
            embedding_model=CANGJIE_LOCAL_MODEL,
        )

        # Verify indexed
        assert store.is_indexed()
        assert store.get_metadata() is not None

        # Clear
        store.clear()

        # Verify cleared
        assert not store.is_indexed()
        assert store.get_metadata() is None
        assert store.get_index() is None

    def test_index_nodes_with_chunker(
        self,
        integration_docs_dir: Path,
        local_settings: Settings,
    ) -> None:
        """Test indexing nodes using chunker."""
        reset_embedding_provider()
        embedding_provider = get_embedding_provider(local_settings)
        store = VectorStore(
            db_path=local_settings.chroma_db_dir,
            embedding_provider=embedding_provider,
        )

        loader = DocumentLoader(integration_docs_dir)
        documents = loader.load_all_documents()

        # Use chunker to create nodes
        chunker = create_chunker(embedding_provider)
        nodes = chunker.chunk_documents(documents, use_semantic=False)

        # Index nodes
        index = store.index_nodes(nodes)

        assert index is not None
        assert store.is_indexed()
        assert store.collection.count() > 0

    def test_get_metadata_after_save(
        self,
        integration_docs_dir: Path,
        local_settings: Settings,
    ) -> None:
        """Test getting metadata after saving."""
        reset_embedding_provider()
        embedding_provider = get_embedding_provider(local_settings)
        store = VectorStore(
            db_path=local_settings.chroma_db_dir,
            embedding_provider=embedding_provider,
        )

        loader = DocumentLoader(integration_docs_dir)
        documents = loader.load_all_documents()
        store.index_documents(documents)

        # Save metadata
        store.save_metadata(
            version="v1.0.0",
            lang="zh",
            embedding_model="test:model",
        )

        # Get metadata
        metadata = store.get_metadata()
        assert metadata is not None
        assert metadata.version == "v1.0.0"
        assert metadata.lang == "zh"
        assert metadata.embedding_model == "test:model"
        assert metadata.document_count > 0

    def test_search_returns_empty_when_not_indexed(
        self,
        temp_data_dir: Path,
    ) -> None:
        """Test search returns empty list when not indexed."""
        reset_embedding_provider()
        settings = Settings(
            docs_version="test",
            docs_lang="zh",
            embedding_type="local",
            local_model=CANGJIE_LOCAL_MODEL,
            data_dir=temp_data_dir,
        )
        embedding_provider = get_embedding_provider(settings)
        store = VectorStore(
            db_path=settings.chroma_db_dir,
            embedding_provider=embedding_provider,
        )

        # Should return empty list
        results = store.search(query="test", top_k=5)
        assert results == []

    def test_reindex_replaces_existing(
        self,
        integration_docs_dir: Path,
        local_settings: Settings,
    ) -> None:
        """Test reindexing replaces existing index."""
        reset_embedding_provider()
        embedding_provider = get_embedding_provider(local_settings)
        store = VectorStore(
            db_path=local_settings.chroma_db_dir,
            embedding_provider=embedding_provider,
        )

        loader = DocumentLoader(integration_docs_dir)
        documents = loader.load_all_documents()

        # Index first time
        store.index_documents(documents)
        count1 = store.collection.count()

        # Index again (should replace)
        store.index_documents(documents)
        count2 = store.collection.count()

        # Count should be same (replaced, not added)
        assert count1 == count2

    def test_version_matches_returns_false_for_different_version(
        self,
        integration_docs_dir: Path,
        local_settings: Settings,
    ) -> None:
        """Test version_matches with different version."""
        reset_embedding_provider()
        embedding_provider = get_embedding_provider(local_settings)
        store = VectorStore(
            db_path=local_settings.chroma_db_dir,
            embedding_provider=embedding_provider,
        )

        loader = DocumentLoader(integration_docs_dir)
        documents = loader.load_all_documents()
        store.index_documents(documents)
        store.save_metadata(
            version="v1.0.0",
            lang="zh",
            embedding_model=CANGJIE_LOCAL_MODEL,
        )

        # Should not match different version
        assert not store.version_matches("v2.0.0", "zh")
        assert not store.version_matches("v1.0.0", "en")
        assert store.version_matches("v1.0.0", "zh")

    def test_search_with_different_top_k(
        self,
        local_indexed_store: VectorStore,
    ) -> None:
        """Test search with different top_k values."""
        # Top 1
        results1 = local_indexed_store.search(query="函数", top_k=1)
        assert len(results1) == 1

        # Top 3
        results3 = local_indexed_store.search(query="函数", top_k=3)
        assert len(results3) <= 3

        # Top 10 (may return fewer if not enough matches)
        results10 = local_indexed_store.search(query="函数", top_k=10)
        assert len(results10) >= len(results3)
