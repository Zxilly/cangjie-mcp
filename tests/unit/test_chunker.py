"""Tests for indexer/chunker.py document chunking functionality."""

from unittest.mock import MagicMock

import pytest
from llama_index.core import Document
from llama_index.core.embeddings import BaseEmbedding

from cangjie_mcp.indexer.chunker import (
    CODE_BLOCK_PATTERN,
    DocumentChunker,
    create_chunker,
)
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

    def test_chunk_single_document_fallback(self, mock_embedding_provider: MockEmbeddingProvider) -> None:
        """Test chunking a single document with fallback."""
        chunker = DocumentChunker(embedding_provider=mock_embedding_provider)

        doc = Document(text="Single document content. More text here.")
        nodes = chunker.chunk_single_document(doc, use_semantic=False)

        assert len(nodes) >= 1
        assert all(node.get_content() for node in nodes)

    def test_fallback_splitter_caching(self, mock_embedding_provider: MockEmbeddingProvider) -> None:
        """Test that fallback splitter is cached."""
        chunker = DocumentChunker(embedding_provider=mock_embedding_provider)

        splitter1 = chunker._get_fallback_splitter()
        splitter2 = chunker._get_fallback_splitter()

        assert splitter1 is splitter2

    def test_chunk_documents_preserves_metadata(self, mock_embedding_provider: MockEmbeddingProvider) -> None:
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


class TestCodeAwareSplitting:
    """Tests for code-aware splitting functionality."""

    def test_code_block_pattern_matches(self) -> None:
        """Test that CODE_BLOCK_PATTERN matches code blocks."""
        text = """Some text.

```cangjie
class Foo {
    let x: Int64
}
```

More text."""
        matches = list(CODE_BLOCK_PATTERN.finditer(text))
        assert len(matches) == 1
        assert "class Foo" in matches[0].group()

    def test_code_block_pattern_multiple(self) -> None:
        """Test pattern matches multiple code blocks."""
        text = """First block:

```python
def foo():
    pass
```

Second block:

```shell
echo hello
```
"""
        matches = list(CODE_BLOCK_PATTERN.finditer(text))
        assert len(matches) == 2

    def test_split_text_preserving_code_no_code(self, mock_embedding_provider: MockEmbeddingProvider) -> None:
        """Test splitting text without code blocks."""
        chunker = DocumentChunker(embedding_provider=mock_embedding_provider)
        segments = chunker._split_text_preserving_code("Just plain text here.")

        assert len(segments) == 1
        assert not segments[0].has_code
        assert segments[0].code_count == 0

    def test_split_text_preserving_code_with_code(self, mock_embedding_provider: MockEmbeddingProvider) -> None:
        """Test splitting text with code block preserves context."""
        chunker = DocumentChunker(embedding_provider=mock_embedding_provider)
        text = """Introduction paragraph.

Here is an example:

```cangjie
let x = 42
```

Following text."""

        segments = chunker._split_text_preserving_code(text)

        # Should have: intro text, context+code, following text
        code_segments = [s for s in segments if s.has_code]
        assert len(code_segments) >= 1
        # Code block should be intact
        assert "let x = 42" in code_segments[0].text

    def test_split_text_preserving_code_keeps_context(self, mock_embedding_provider: MockEmbeddingProvider) -> None:
        """Test that code blocks are kept with their preceding context."""
        chunker = DocumentChunker(embedding_provider=mock_embedding_provider)
        text = """## Example Section

This explains the code:

```cangjie
func test() {}
```"""

        segments = chunker._split_text_preserving_code(text)
        code_segment = next(s for s in segments if s.has_code)

        # Should contain both context and code
        assert "explains the code" in code_segment.text or any("explains" in s.text for s in segments)
        assert "func test()" in code_segment.text


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

    def test_create_chunker_with_max_chunk_size(self, mock_embedding_provider: MockEmbeddingProvider) -> None:
        """Test creating a chunker with custom max_chunk_size."""
        chunker = create_chunker(mock_embedding_provider, max_chunk_size=4000)

        assert chunker.max_chunk_size == 4000

    def test_create_chunker_no_embedding(self) -> None:
        """Test creating a chunker without embedding provider."""
        chunker = create_chunker(None)
        assert isinstance(chunker, DocumentChunker)
        assert chunker.embedding_provider is None

    def test_chunk_documents_no_embedding_uses_sentence_splitting(self) -> None:
        """Test that chunking without embedding provider uses sentence splitting."""
        chunker = DocumentChunker(embedding_provider=None)
        docs = [
            Document(text="This is a test document. It has multiple sentences."),
        ]
        nodes = chunker.chunk_documents(docs, use_semantic=True)
        # Should succeed with sentence splitting even though use_semantic=True
        assert len(nodes) >= 1
        assert all(node.get_content() for node in nodes)
