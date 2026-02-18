"""Tests for HTTP query server."""

from unittest.mock import AsyncMock, MagicMock

import pytest
from starlette.testclient import TestClient

from cangjie_mcp.indexer.store import IndexMetadata, SearchResult, SearchResultMetadata
from cangjie_mcp.server.http import create_http_app


@pytest.fixture
def mock_search_index():
    index = MagicMock()
    index.query = AsyncMock(
        return_value=[
            SearchResult(
                text="test content",
                score=0.95,
                metadata=SearchResultMetadata(
                    file_path="basics/test.md",
                    category="basics",
                    topic="test",
                    title="Test Title",
                ),
            ),
        ]
    )
    return index


@pytest.fixture
def mock_document_source():
    source = MagicMock()
    source.get_categories.return_value = ["basics", "stdlib"]
    source.get_topics_in_category.side_effect = lambda cat: {  # type: ignore[no-untyped-def]
        "basics": ["hello_world", "variables"],
        "stdlib": ["collections"],
    }.get(cat, [])
    source.get_topic_titles.side_effect = lambda cat: {  # type: ignore[no-untyped-def]
        "basics": {"hello_world": "Hello World", "variables": "Variables"},
        "stdlib": {"collections": "Collections"},
    }.get(cat, {})

    mock_doc = MagicMock()
    mock_doc.text = "# Hello World\nSample content"
    mock_doc.metadata = {
        "file_path": "basics/hello_world.md",
        "category": "basics",
        "topic": "hello_world",
        "title": "Hello World",
    }
    source.get_document_by_topic.return_value = mock_doc
    return source


@pytest.fixture
def index_metadata():
    return IndexMetadata(
        version="v1.0.0",
        lang="zh",
        embedding_model="local:test-model",
        document_count=42,
    )


@pytest.fixture
def client(mock_search_index, mock_document_source, index_metadata):
    app = create_http_app(mock_search_index, mock_document_source, index_metadata)
    return TestClient(app)


class TestHealthEndpoint:
    def test_health_returns_ok(self, client):
        resp = client.get("/health")
        assert resp.status_code == 200
        assert resp.json() == {"status": "ok"}


class TestInfoEndpoint:
    def test_info_returns_metadata(self, client):
        resp = client.get("/info")
        assert resp.status_code == 200
        data = resp.json()
        assert data["version"] == "v1.0.0"
        assert data["lang"] == "zh"
        assert data["embedding_model"] == "local:test-model"
        assert data["document_count"] == 42


class TestSearchEndpoint:
    def test_search_returns_results(self, client, mock_search_index):
        resp = client.post("/search", json={"query": "hello", "top_k": 3})
        assert resp.status_code == 200
        data = resp.json()
        assert len(data["results"]) == 1
        assert data["results"][0]["text"] == "test content"
        assert data["results"][0]["score"] == 0.95
        assert data["results"][0]["metadata"]["category"] == "basics"

    def test_search_requires_query(self, client):
        resp = client.post("/search", json={})
        assert resp.status_code == 400
        assert "query" in resp.json()["error"]

    def test_search_passes_category(self, client, mock_search_index):
        client.post("/search", json={"query": "test", "category": "stdlib"})
        mock_search_index.query.assert_called_once()
        call_kwargs = mock_search_index.query.call_args
        assert call_kwargs.kwargs.get("category") == "stdlib"


class TestTopicsEndpoint:
    def test_topics_returns_categories(self, client):
        resp = client.get("/topics")
        assert resp.status_code == 200
        data = resp.json()
        assert "basics" in data["categories"]
        topic_names = [t["name"] for t in data["categories"]["basics"]]
        assert "hello_world" in topic_names

    def test_topics_returns_topic_titles(self, client):
        resp = client.get("/topics")
        data = resp.json()
        for _cat, topics in data["categories"].items():
            for topic_info in topics:
                assert "name" in topic_info
                assert "title" in topic_info

    def test_topics_returns_all_categories(self, client):
        resp = client.get("/topics")
        data = resp.json()
        assert set(data["categories"].keys()) == {"basics", "stdlib"}


class TestTopicDetailEndpoint:
    def test_topic_detail_returns_content(self, client):
        resp = client.get("/topics/basics/hello_world")
        assert resp.status_code == 200
        data = resp.json()
        assert "Hello World" in data["content"]
        assert data["category"] == "basics"

    def test_topic_detail_not_found(self, client, mock_document_source):
        mock_document_source.get_document_by_topic.return_value = None
        resp = client.get("/topics/unknown/missing")
        assert resp.status_code == 404
