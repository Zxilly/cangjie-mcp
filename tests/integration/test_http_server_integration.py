"""Integration tests for the HTTP query server.

Tests the HTTP server endpoints with real indexed data, verifying
the full request-response cycle through SearchIndex and DocumentSource.
"""

import pytest
from starlette.testclient import TestClient

from cangjie_mcp.config import Settings
from cangjie_mcp.indexer.store import IndexMetadata, VectorStore
from cangjie_mcp.server.http import create_http_app
from tests.integration.conftest import TestDocumentSource, VectorStoreSearchIndex


@pytest.fixture(scope="session")
def http_client(
    local_indexed_store: VectorStore,
    test_doc_source: TestDocumentSource,
    shared_local_settings: Settings,
) -> TestClient:
    """Session-scoped HTTP test client backed by real indexed data."""
    search_index = VectorStoreSearchIndex(local_indexed_store)
    metadata = IndexMetadata(
        version=shared_local_settings.docs_version,
        lang=shared_local_settings.docs_lang,
        embedding_model=f"local:{shared_local_settings.local_model}",
        document_count=local_indexed_store.collection.count(),
    )
    app = create_http_app(search_index, test_doc_source, metadata)
    return TestClient(app)


class TestHealthEndpoint:
    """Test /health endpoint with real server."""

    def test_returns_ok(self, http_client: TestClient) -> None:
        resp = http_client.get("/health")
        assert resp.status_code == 200
        assert resp.json() == {"status": "ok"}


class TestInfoEndpoint:
    """Test /info endpoint with real index metadata."""

    def test_returns_metadata(self, http_client: TestClient, shared_local_settings: Settings) -> None:
        resp = http_client.get("/info")
        assert resp.status_code == 200
        data = resp.json()
        assert data["version"] == shared_local_settings.docs_version
        assert data["lang"] == shared_local_settings.docs_lang
        assert data["document_count"] > 0
        assert "embedding_model" in data


class TestSearchEndpoint:
    """Test /search endpoint with real vector search."""

    def test_search_returns_results(self, http_client: TestClient) -> None:
        resp = http_client.post("/search", json={"query": "变量声明", "top_k": 3})
        assert resp.status_code == 200
        data = resp.json()
        assert len(data["results"]) > 0
        for item in data["results"]:
            assert "text" in item
            assert "score" in item
            assert item["score"] > 0
            assert "metadata" in item
            assert "category" in item["metadata"]
            assert "topic" in item["metadata"]

    def test_search_with_category_filter(self, http_client: TestClient) -> None:
        resp = http_client.post("/search", json={"query": "编译", "category": "tools", "top_k": 3})
        assert resp.status_code == 200
        data = resp.json()
        assert len(data["results"]) > 0
        for item in data["results"]:
            assert item["metadata"]["category"] == "tools"

    def test_search_empty_query_returns_400(self, http_client: TestClient) -> None:
        resp = http_client.post("/search", json={"query": "", "top_k": 3})
        assert resp.status_code == 400

    def test_search_missing_query_returns_400(self, http_client: TestClient) -> None:
        resp = http_client.post("/search", json={"top_k": 3})
        assert resp.status_code == 400

    def test_search_respects_top_k(self, http_client: TestClient) -> None:
        resp = http_client.post("/search", json={"query": "仓颉", "top_k": 2})
        assert resp.status_code == 200
        data = resp.json()
        assert len(data["results"]) <= 2


class TestTopicsEndpoint:
    """Test /topics endpoint with real document source."""

    def test_lists_all_categories(self, http_client: TestClient) -> None:
        resp = http_client.get("/topics")
        assert resp.status_code == 200
        data = resp.json()
        categories = data["categories"]
        assert "basics" in categories
        assert "syntax" in categories
        assert "tools" in categories

    def test_lists_topics_in_categories(self, http_client: TestClient) -> None:
        resp = http_client.get("/topics")
        data = resp.json()
        assert "hello_world" in data["categories"]["basics"]
        assert "variables" in data["categories"]["basics"]
        assert "functions" in data["categories"]["syntax"]
        assert "cjc" in data["categories"]["tools"]
        assert "cjpm" in data["categories"]["tools"]


class TestTopicDetailEndpoint:
    """Test /topics/{category}/{topic} endpoint with real documents."""

    def test_returns_document_content(self, http_client: TestClient) -> None:
        resp = http_client.get("/topics/basics/hello_world")
        assert resp.status_code == 200
        data = resp.json()
        assert "Hello World" in data["content"] or "Hello, Cangjie" in data["content"]
        assert data["category"] == "basics"
        assert data["topic"] == "hello_world"

    def test_returns_404_for_missing_topic(self, http_client: TestClient) -> None:
        resp = http_client.get("/topics/basics/nonexistent_topic")
        assert resp.status_code == 404

    def test_returns_404_for_missing_category(self, http_client: TestClient) -> None:
        resp = http_client.get("/topics/nonexistent_category/hello_world")
        assert resp.status_code == 404

    def test_returns_tool_documentation(self, http_client: TestClient) -> None:
        resp = http_client.get("/topics/tools/cjpm")
        assert resp.status_code == 200
        data = resp.json()
        assert "cjpm" in data["content"].lower()
        assert data["category"] == "tools"
        assert data["topic"] == "cjpm"


class TestSearchAndBrowseWorkflow:
    """End-to-end workflow: search, then browse topics found."""

    def test_search_then_get_topic(self, http_client: TestClient) -> None:
        """Search for a term, then retrieve the full document for the top result."""
        search_resp = http_client.post("/search", json={"query": "函数定义", "top_k": 1})
        assert search_resp.status_code == 200
        results = search_resp.json()["results"]
        assert len(results) > 0

        top = results[0]
        category = top["metadata"]["category"]
        topic = top["metadata"]["topic"]

        detail_resp = http_client.get(f"/topics/{category}/{topic}")
        assert detail_resp.status_code == 200
        detail = detail_resp.json()
        assert len(detail["content"]) > 0

    def test_list_then_get_all_topics(self, http_client: TestClient) -> None:
        """List all topics, then verify each one is retrievable."""
        topics_resp = http_client.get("/topics")
        assert topics_resp.status_code == 200
        categories = topics_resp.json()["categories"]

        for category, topic_list in categories.items():
            for topic in topic_list:
                resp = http_client.get(f"/topics/{category}/{topic}")
                assert resp.status_code == 200, f"Failed to get {category}/{topic}"
                assert len(resp.json()["content"]) > 0
