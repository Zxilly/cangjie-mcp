"""Tests for document loader."""

from pathlib import Path

from cangjie_mcp.indexer.loader import (
    DocumentLoader,
    extract_code_blocks,
    extract_title_from_content,
)


class TestExtractTitle:
    """Tests for title extraction."""

    def test_extract_h1_title(self) -> None:
        """Test extracting H1 title."""
        content = "# Hello World\n\nSome content"
        assert extract_title_from_content(content) == "Hello World"

    def test_no_title(self) -> None:
        """Test when no title present."""
        content = "Some content without a heading"
        assert extract_title_from_content(content) == ""

    def test_h2_not_extracted(self) -> None:
        """Test that H2 is not extracted as title."""
        content = "## This is H2\n\nContent"
        assert extract_title_from_content(content) == ""


class TestExtractCodeBlocks:
    """Tests for code block extraction."""

    def test_extract_single_block(self, sample_markdown_content: str) -> None:
        """Test extracting a single code block."""
        blocks = extract_code_blocks(sample_markdown_content)
        assert len(blocks) >= 1
        assert any(b.language == "cangjie" for b in blocks)

    def test_extract_multiple_blocks(self, sample_markdown_content: str) -> None:
        """Test extracting multiple code blocks."""
        blocks = extract_code_blocks(sample_markdown_content)
        assert len(blocks) == 2
        languages = [b.language for b in blocks]
        assert "cangjie" in languages
        assert "bash" in languages

    def test_code_content(self, sample_markdown_content: str) -> None:
        """Test code block content."""
        blocks = extract_code_blocks(sample_markdown_content)
        cangjie_block = next(b for b in blocks if b.language == "cangjie")
        assert 'println("Hello, Cangjie!")' in cangjie_block.code


class TestDocumentLoader:
    """Tests for DocumentLoader class."""

    def test_load_all_documents(self, sample_docs_dir: Path) -> None:
        """Test loading all documents."""
        loader = DocumentLoader(sample_docs_dir)
        docs = loader.load_all_documents()
        assert len(docs) == 2

    def test_get_categories(self, sample_docs_dir: Path) -> None:
        """Test getting categories."""
        loader = DocumentLoader(sample_docs_dir)
        categories = loader.get_categories()
        assert "basics" in categories
        assert "tools" in categories

    def test_get_topics_in_category(self, sample_docs_dir: Path) -> None:
        """Test getting topics in a category."""
        loader = DocumentLoader(sample_docs_dir)
        topics = loader.get_topics_in_category("basics")
        assert "hello_world" in topics

    def test_get_document_by_topic(self, sample_docs_dir: Path) -> None:
        """Test getting a document by topic."""
        loader = DocumentLoader(sample_docs_dir)
        doc = loader.get_document_by_topic("hello_world")
        assert doc is not None
        assert "Sample Topic" in doc.text

    def test_get_document_not_found(self, sample_docs_dir: Path) -> None:
        """Test getting a non-existent document."""
        loader = DocumentLoader(sample_docs_dir)
        doc = loader.get_document_by_topic("nonexistent")
        assert doc is None
