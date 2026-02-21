"""Tests for BM25Store."""

from pathlib import Path

import pytest
from llama_index.core.schema import TextNode

from cangjie_mcp.indexer.bm25_store import BM25Store, _cangjie_splitter


class TestCangjieTokenizer:
    """Tests for the jieba-based tokenizer."""

    def test_chinese_text(self) -> None:
        tokens = _cangjie_splitter("仓颉编程语言")
        assert len(tokens) > 0
        assert all(isinstance(t, str) for t in tokens)

    def test_english_text(self) -> None:
        tokens = _cangjie_splitter("Hello World")
        assert "hello" in tokens
        assert "world" in tokens

    def test_mixed_text(self) -> None:
        tokens = _cangjie_splitter("仓颉语言 BM25 搜索")
        assert len(tokens) > 0
        # Should have both Chinese and English tokens
        assert "bm25" in tokens

    def test_empty_text(self) -> None:
        tokens = _cangjie_splitter("")
        assert tokens == []

    def test_whitespace_only(self) -> None:
        tokens = _cangjie_splitter("   \n\t  ")
        assert tokens == []


class TestBM25Store:
    """Tests for BM25Store class."""

    @pytest.fixture
    def bm25_dir(self, temp_data_dir: Path) -> Path:
        return temp_data_dir / "bm25_index"

    @pytest.fixture
    def sample_nodes(self) -> list[TextNode]:
        return [
            TextNode(
                text="仓颉是一种现代编程语言，支持函数式和面向对象编程。",
                metadata={"file_path": "basics/intro.md", "category": "basics", "topic": "intro", "title": "介绍"},
            ),
            TextNode(
                text="Pattern matching in Cangjie uses the match keyword.",
                metadata={
                    "file_path": "syntax/pattern.md",
                    "category": "syntax",
                    "topic": "pattern",
                    "title": "Pattern Matching",
                },
            ),
            TextNode(
                text="BM25 is a ranking function used in information retrieval.",
                metadata={
                    "file_path": "tools/search.md",
                    "category": "tools",
                    "topic": "search",
                    "title": "Search",
                },
            ),
        ]

    def test_not_indexed_initially(self, bm25_dir: Path) -> None:
        store = BM25Store(bm25_dir)
        assert not store.is_indexed()

    def test_build_and_is_indexed(self, bm25_dir: Path, sample_nodes: list[TextNode]) -> None:
        store = BM25Store(bm25_dir)
        store.build_from_nodes(sample_nodes)
        assert store.is_indexed()

    def test_build_empty_nodes(self, bm25_dir: Path) -> None:
        store = BM25Store(bm25_dir)
        store.build_from_nodes([])
        assert not store.is_indexed()

    def test_search_after_build(self, bm25_dir: Path, sample_nodes: list[TextNode]) -> None:
        store = BM25Store(bm25_dir)
        store.build_from_nodes(sample_nodes)

        results = store.search("pattern matching", top_k=2)
        assert len(results) > 0
        assert any("pattern" in r.text.lower() or "match" in r.text.lower() for r in results)

    def test_search_chinese(self, bm25_dir: Path, sample_nodes: list[TextNode]) -> None:
        store = BM25Store(bm25_dir)
        store.build_from_nodes(sample_nodes)

        results = store.search("编程语言", top_k=2)
        assert len(results) > 0
        assert any("编程" in r.text for r in results)

    def test_search_with_category_filter(self, bm25_dir: Path, sample_nodes: list[TextNode]) -> None:
        store = BM25Store(bm25_dir)
        store.build_from_nodes(sample_nodes)

        results = store.search("pattern", top_k=5, category="syntax")
        for r in results:
            assert r.metadata.category == "syntax"

    def test_search_empty_query(self, bm25_dir: Path, sample_nodes: list[TextNode]) -> None:
        store = BM25Store(bm25_dir)
        store.build_from_nodes(sample_nodes)

        results = store.search("", top_k=5)
        assert results == []

    def test_search_without_build(self, bm25_dir: Path) -> None:
        store = BM25Store(bm25_dir)
        results = store.search("test", top_k=5)
        assert results == []

    def test_persistence_load(self, bm25_dir: Path, sample_nodes: list[TextNode]) -> None:
        # Build
        store1 = BM25Store(bm25_dir)
        store1.build_from_nodes(sample_nodes)

        # Load in a new instance
        store2 = BM25Store(bm25_dir)
        assert store2.load()

        results = store2.search("BM25 ranking", top_k=2)
        assert len(results) > 0

    def test_load_nonexistent(self, bm25_dir: Path) -> None:
        store = BM25Store(bm25_dir)
        assert not store.load()

    def test_clear(self, bm25_dir: Path, sample_nodes: list[TextNode]) -> None:
        store = BM25Store(bm25_dir)
        store.build_from_nodes(sample_nodes)
        assert store.is_indexed()

        store.clear()
        assert not store.is_indexed()
        assert not bm25_dir.exists()

    def test_metadata_preserved(self, bm25_dir: Path, sample_nodes: list[TextNode]) -> None:
        store = BM25Store(bm25_dir)
        store.build_from_nodes(sample_nodes)

        results = store.search("pattern matching", top_k=1)
        assert len(results) == 1
        assert results[0].metadata.file_path == "syntax/pattern.md"
        assert results[0].metadata.category == "syntax"
        assert results[0].metadata.topic == "pattern"
        assert results[0].metadata.title == "Pattern Matching"
