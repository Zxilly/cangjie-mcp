"""Tests for indexer/store.py Pydantic models and VectorStore."""

import json
from pathlib import Path

from cangjie_mcp.indexer.store import (
    IndexMetadata,
    SearchResult,
    SearchResultMetadata,
)


class TestIndexMetadata:
    """Tests for IndexMetadata Pydantic model."""

    def test_create_metadata(self) -> None:
        """Test creating IndexMetadata."""
        metadata = IndexMetadata(
            version="v1.0.7",
            lang="zh",
            embedding_model="BAAI/bge-small-zh-v1.5",
            document_count=100,
        )
        assert metadata.version == "v1.0.7"
        assert metadata.lang == "zh"
        assert metadata.embedding_model == "BAAI/bge-small-zh-v1.5"
        assert metadata.document_count == 100

    def test_json_serialization(self) -> None:
        """Test JSON serialization and deserialization."""
        metadata = IndexMetadata(
            version="v1.0.7",
            lang="en",
            embedding_model="text-embedding-3-small",
            document_count=50,
        )
        json_str = metadata.model_dump_json()
        parsed = IndexMetadata.model_validate_json(json_str)
        assert parsed.version == metadata.version
        assert parsed.lang == metadata.lang
        assert parsed.embedding_model == metadata.embedding_model
        assert parsed.document_count == metadata.document_count

    def test_json_file_roundtrip(self, temp_data_dir: Path) -> None:
        """Test saving and loading from JSON file."""
        metadata = IndexMetadata(
            version="latest",
            lang="zh",
            embedding_model="BAAI/bge-small-zh-v1.5",
            document_count=200,
        )
        file_path = temp_data_dir / "metadata.json"
        file_path.write_text(metadata.model_dump_json(indent=2), encoding="utf-8")

        loaded = IndexMetadata.model_validate_json(file_path.read_text(encoding="utf-8"))
        assert loaded.version == "latest"
        assert loaded.document_count == 200


class TestSearchResultMetadata:
    """Tests for SearchResultMetadata Pydantic model."""

    def test_default_values(self) -> None:
        """Test default values for optional fields."""
        metadata = SearchResultMetadata()
        assert metadata.file_path == ""
        assert metadata.category == ""
        assert metadata.topic == ""
        assert metadata.title == ""

    def test_with_values(self) -> None:
        """Test with all values provided."""
        metadata = SearchResultMetadata(
            file_path="/docs/basics/hello.md",
            category="basics",
            topic="hello",
            title="Hello World",
        )
        assert metadata.file_path == "/docs/basics/hello.md"
        assert metadata.category == "basics"
        assert metadata.topic == "hello"
        assert metadata.title == "Hello World"


class TestSearchResult:
    """Tests for SearchResult Pydantic model."""

    def test_create_result(self) -> None:
        """Test creating SearchResult."""
        metadata = SearchResultMetadata(
            file_path="/docs/syntax/functions.md",
            category="syntax",
            topic="functions",
            title="Functions",
        )
        result = SearchResult(
            text="Functions are defined using the func keyword.",
            score=0.95,
            metadata=metadata,
        )
        assert result.text == "Functions are defined using the func keyword."
        assert result.score == 0.95
        assert result.metadata.category == "syntax"

    def test_nested_json_serialization(self) -> None:
        """Test nested JSON serialization."""
        result = SearchResult(
            text="Sample text",
            score=0.8,
            metadata=SearchResultMetadata(
                file_path="/test.md",
                category="test",
                topic="test_topic",
                title="Test",
            ),
        )
        json_str = result.model_dump_json()
        data = json.loads(json_str)
        assert data["text"] == "Sample text"
        assert data["score"] == 0.8
        assert data["metadata"]["category"] == "test"

    def test_attribute_access(self) -> None:
        """Test attribute access pattern (replaces .get() pattern)."""
        result = SearchResult(
            text="Content here",
            score=0.75,
            metadata=SearchResultMetadata(
                file_path="/docs/api.md",
                category="api",
            ),
        )
        # This is the pattern we use now instead of result["text"]
        assert result.text == "Content here"
        assert result.score == 0.75
        assert result.metadata.file_path == "/docs/api.md"
        assert result.metadata.category == "api"
        # Empty defaults
        assert result.metadata.topic == ""
        assert result.metadata.title == ""
