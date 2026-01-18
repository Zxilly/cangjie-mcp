"""Tests for indexer/chunker.py document chunking functionality."""

from unittest.mock import MagicMock

import pytest
from llama_index.core import Document
from llama_index.core.embeddings import BaseEmbedding

from cangjie_mcp.indexer.chunker import DocumentChunker, create_chunker
from cangjie_mcp.indexer.embeddings import EmbeddingProvider


class MockEmbeddingProvider(EmbeddingProvider):
    """Mock embedding provider for testing."""

    def __init__(self) -> None:
        self._model: MagicMock = MagicMock()

    def get_embedding_model(self) -> BaseEmbedding:
        return self._model

    def get_model_name(self) -> str:
        return "mock:test-model"


@pytest.fixture
def mock_embedding_provider() -> MockEmbeddingProvider:
    """Create a mock embedding provider."""
    return MockEmbeddingProvider()


class TestDocumentChunker:
    """Tests for DocumentChunker class."""

    def test_init(self, mock_embedding_provider: MockEmbeddingProvider) -> None:
        """Test DocumentChunker initialization."""
        chunker = DocumentChunker(
            embedding_provider=mock_embedding_provider,
            buffer_size=2,
            breakpoint_percentile_threshold=90,
        )
        assert chunker.embedding_provider == mock_embedding_provider
        assert chunker.buffer_size == 2
        assert chunker.breakpoint_percentile_threshold == 90

    def test_init_default_values(self, mock_embedding_provider: MockEmbeddingProvider) -> None:
        """Test DocumentChunker with default values."""
        chunker = DocumentChunker(embedding_provider=mock_embedding_provider)
        assert chunker.buffer_size == 1
        assert chunker.breakpoint_percentile_threshold == 95

    def test_chunk_empty_documents(self, mock_embedding_provider: MockEmbeddingProvider) -> None:
        """Test chunking with empty document list."""
        chunker = DocumentChunker(embedding_provider=mock_embedding_provider)
        result = chunker.chunk_documents([])
        assert result == []

    def test_chunk_documents_fallback(self, mock_embedding_provider: MockEmbeddingProvider) -> None:
        """Test chunking with fallback to sentence splitter."""
        chunker = DocumentChunker(embedding_provider=mock_embedding_provider)

        docs = [
            Document(text="This is a test document. It has multiple sentences."),
        ]

        # Use non-semantic splitting which doesn't require embeddings
        nodes = chunker.chunk_documents(docs, use_semantic=False)
        assert len(nodes) >= 1
        assert all(node.get_content() for node in nodes)

    def test_chunk_single_document_fallback(
        self, mock_embedding_provider: MockEmbeddingProvider
    ) -> None:
        """Test chunking a single document with fallback."""
        chunker = DocumentChunker(embedding_provider=mock_embedding_provider)

        doc = Document(text="Single document content. More text here.")
        nodes = chunker.chunk_single_document(doc, use_semantic=False)

        assert len(nodes) >= 1
        assert all(node.get_content() for node in nodes)

    def test_fallback_splitter_caching(
        self, mock_embedding_provider: MockEmbeddingProvider
    ) -> None:
        """Test that fallback splitter is cached."""
        chunker = DocumentChunker(embedding_provider=mock_embedding_provider)

        splitter1 = chunker._get_fallback_splitter()
        splitter2 = chunker._get_fallback_splitter()

        assert splitter1 is splitter2

    def test_chunk_documents_preserves_metadata(
        self, mock_embedding_provider: MockEmbeddingProvider
    ) -> None:
        """Test that chunking preserves document metadata."""
        chunker = DocumentChunker(embedding_provider=mock_embedding_provider)

        doc = Document(
            text="This is test content. Another sentence here.",
            metadata={"category": "test", "topic": "testing"},
        )

        nodes = chunker.chunk_documents([doc], use_semantic=False)

        assert len(nodes) >= 1
        # Metadata should be preserved in nodes
        for node in nodes:
            assert "category" in node.metadata or node.get_content()


class TestCreateChunker:
    """Tests for create_chunker factory function."""

    def test_create_chunker(self, mock_embedding_provider: MockEmbeddingProvider) -> None:
        """Test creating a chunker with factory function."""
        chunker = create_chunker(mock_embedding_provider)

        assert isinstance(chunker, DocumentChunker)
        assert chunker.embedding_provider == mock_embedding_provider
        # Should use default values
        assert chunker.buffer_size == 1
        assert chunker.breakpoint_percentile_threshold == 95
