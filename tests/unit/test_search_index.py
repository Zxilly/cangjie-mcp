"""Tests for SearchIndex abstraction."""

from pathlib import Path
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from cangjie_mcp.config import IndexInfo
from cangjie_mcp.indexer.search_index import LocalSearchIndex, RemoteSearchIndex


class TestLocalSearchIndex:
    """Tests for LocalSearchIndex."""

    @patch("cangjie_mcp.indexer.store.create_vector_store")
    @patch("cangjie_mcp.indexer.bm25_store.BM25Store")
    @patch("cangjie_mcp.indexer.initializer.initialize_and_index")
    def test_init_returns_index_info(
        self,
        mock_init_and_index: MagicMock,
        mock_bm25_class: MagicMock,
        mock_create_store: MagicMock,
    ) -> None:
        """Test that init() returns IndexInfo."""
        mock_settings = MagicMock()
        mock_settings.has_embedding = True
        expected_info = IndexInfo(
            version="v1.0.0",
            lang="zh",
            embedding_model_name="local:test",
            data_dir=Path("/test"),
        )
        mock_init_and_index.return_value = expected_info

        # BM25 store mock
        mock_bm25 = MagicMock()
        mock_bm25.load.return_value = True
        mock_bm25_class.return_value = mock_bm25

        index = LocalSearchIndex(mock_settings)
        result = index.init()

        assert result is expected_info
        mock_init_and_index.assert_called_once_with(mock_settings)
        mock_create_store.assert_called_once()

    @patch("cangjie_mcp.indexer.store.create_vector_store")
    @patch("cangjie_mcp.indexer.bm25_store.BM25Store")
    @patch("cangjie_mcp.indexer.initializer.initialize_and_index")
    def test_init_bm25_only_mode(
        self,
        mock_init_and_index: MagicMock,
        mock_bm25_class: MagicMock,
        mock_create_store: MagicMock,
    ) -> None:
        """Test that init() in BM25-only mode does not create vector store."""
        mock_settings = MagicMock()
        mock_settings.has_embedding = False
        expected_info = IndexInfo(
            version="v1.0.0",
            lang="zh",
            embedding_model_name="none",
            data_dir=Path("/test"),
        )
        mock_init_and_index.return_value = expected_info

        mock_bm25 = MagicMock()
        mock_bm25.load.return_value = True
        mock_bm25_class.return_value = mock_bm25

        index = LocalSearchIndex(mock_settings)
        result = index.init()

        assert result is expected_info
        mock_create_store.assert_not_called()

    @pytest.mark.asyncio
    @patch("cangjie_mcp.indexer.store.create_vector_store", return_value=None)
    @patch("cangjie_mcp.indexer.bm25_store.BM25Store")
    @patch("cangjie_mcp.indexer.initializer.initialize_and_index")
    async def test_query_bm25_only(
        self,
        mock_init_and_index: MagicMock,
        mock_bm25_class: MagicMock,
        mock_create_store: MagicMock,
    ) -> None:
        """Test query in BM25-only mode delegates to BM25Store."""
        from cangjie_mcp.indexer.store import SearchResult, SearchResultMetadata

        mock_settings = MagicMock()
        mock_settings.has_embedding = False
        mock_init_and_index.return_value = IndexInfo(
            version="v1.0.0",
            lang="zh",
            embedding_model_name="none",
            data_dir=Path("/test"),
        )

        expected_results = [
            SearchResult(
                text="bm25 result",
                score=1.5,
                metadata=SearchResultMetadata(
                    file_path="test.md",
                    category="test",
                    topic="topic",
                    title="Title",
                ),
            ),
        ]
        mock_bm25 = MagicMock()
        mock_bm25.load.return_value = True
        mock_bm25.search.return_value = expected_results
        mock_bm25_class.return_value = mock_bm25

        index = LocalSearchIndex(mock_settings)
        index.init()

        results = await index.query("test query", top_k=3)
        assert len(results) == 1
        assert results[0].text == "bm25 result"

    @pytest.mark.asyncio
    async def test_query_returns_empty_without_init(self) -> None:
        """Test that query() returns empty list if init() was not called."""
        mock_settings = MagicMock()
        index = LocalSearchIndex(mock_settings)

        results = await index.query("test")
        assert results == []


class TestRemoteSearchIndex:
    """Tests for RemoteSearchIndex."""

    @patch("httpx.Client")
    def test_init_calls_server_info(self, mock_client_class: MagicMock) -> None:
        """Test that init() calls GET /info on the server."""
        mock_client = MagicMock()
        mock_response = MagicMock()
        mock_response.json.return_value = {
            "version": "v1.0.0",
            "lang": "zh",
            "embedding_model": "local:test",
        }
        mock_client.get.return_value = mock_response
        mock_client.__enter__ = MagicMock(return_value=mock_client)
        mock_client.__exit__ = MagicMock(return_value=False)
        mock_client_class.return_value = mock_client

        index = RemoteSearchIndex("http://localhost:8765")
        result = index.init()

        assert isinstance(result, IndexInfo)
        assert result.version == "v1.0.0"
        assert result.lang == "zh"

    @pytest.mark.asyncio
    @patch("httpx.AsyncClient")
    async def test_query_posts_to_server(self, mock_async_client_class: MagicMock) -> None:
        """Test that query() calls POST /search on the server."""
        mock_client = MagicMock()
        mock_response = MagicMock()
        mock_response.json.return_value = {
            "results": [
                {
                    "text": "remote result",
                    "score": 0.85,
                    "metadata": {
                        "file_path": "remote.md",
                        "category": "cat",
                        "topic": "top",
                        "title": "Title",
                    },
                },
            ],
        }
        mock_response.raise_for_status = MagicMock()
        mock_client.post = AsyncMock(return_value=mock_response)
        mock_client.__aenter__ = AsyncMock(return_value=mock_client)
        mock_client.__aexit__ = AsyncMock(return_value=False)
        mock_async_client_class.return_value = mock_client

        index = RemoteSearchIndex("http://localhost:8765")
        results = await index.query("test query", top_k=5)

        assert len(results) == 1
        assert results[0].text == "remote result"
        assert results[0].score == 0.85
