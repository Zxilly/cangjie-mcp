"""E2E integration tests for the HTTP server.

Starts a real uvicorn server (via the ``http_server_url`` fixture),
sends concurrent requests with ``httpx.AsyncClient``, and verifies
they all complete within a reasonable timeout.  This catches blocking-
I/O bugs that single-request TestClient tests miss.
"""

import asyncio

import httpx

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

TIMEOUT = 30.0  # seconds - generous enough for CI


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


class TestHTTPServerIntegration:
    """Real HTTP server tests using concurrent async requests."""

    async def test_topic_detail_returns_content(self, http_server_url: str) -> None:
        """GET /topics/syntax/functions returns non-empty content with correct metadata."""
        async with httpx.AsyncClient(base_url=http_server_url, timeout=TIMEOUT) as client:
            resp = await client.get("/topics/syntax/functions")

        assert resp.status_code == 200
        data = resp.json()
        assert data["content"]
        assert data["category"] == "syntax"
        assert data["topic"] == "functions"
        assert data["title"]

    async def test_search_returns_results(self, http_server_url: str) -> None:
        """POST /search returns non-empty results with has_code metadata."""
        async with httpx.AsyncClient(base_url=http_server_url, timeout=TIMEOUT) as client:
            resp = await client.post("/search", json={"query": "函数", "top_k": 3})

        assert resp.status_code == 200
        data = resp.json()
        assert len(data["results"]) > 0
        for item in data["results"]:
            assert item["text"]
            assert item["score"] > 0
            assert item["metadata"]["category"]
            assert "has_code" in item["metadata"]

    async def test_concurrent_topics_and_search(self, http_server_url: str) -> None:
        """Fire topic + search requests concurrently; both must complete.

        This is the key test that catches blocking I/O in async handlers:
        if a handler blocks the event loop, the second request will hang
        and the gather will time out.
        """
        async with httpx.AsyncClient(base_url=http_server_url, timeout=TIMEOUT) as client:
            topic_coro = client.get("/topics/syntax/functions")
            search_coro = client.post("/search", json={"query": "变量", "top_k": 3})

            topic_resp, search_resp = await asyncio.gather(topic_coro, search_coro)

        assert topic_resp.status_code == 200
        assert search_resp.status_code == 200

        topic_data = topic_resp.json()
        assert topic_data["content"]
        assert topic_data["category"] == "syntax"

        search_data = search_resp.json()
        assert len(search_data["results"]) > 0

    async def test_concurrent_multiple_topics(self, http_server_url: str) -> None:
        """Fire multiple topic requests concurrently."""
        paths = [
            "/topics/syntax/functions",
            "/topics/syntax/pattern_matching",
            "/topics/basics/hello_world",
            "/topics/basics/variables",
            "/topics/tools/cjc",
        ]
        async with httpx.AsyncClient(base_url=http_server_url, timeout=TIMEOUT) as client:
            responses = await asyncio.gather(*(client.get(p) for p in paths))

        for resp in responses:
            assert resp.status_code == 200
            data = resp.json()
            assert data["content"]
            assert data["category"]
            assert data["topic"]

    async def test_topics_list_includes_titles(self, http_server_url: str) -> None:
        """GET /topics returns categories with TopicInfo objects (name + title)."""
        async with httpx.AsyncClient(base_url=http_server_url, timeout=TIMEOUT) as client:
            resp = await client.get("/topics")

        assert resp.status_code == 200
        data = resp.json()
        assert "categories" in data
        for _cat, topics in data["categories"].items():
            assert isinstance(topics, list)
            for topic_info in topics:
                assert "name" in topic_info
                assert "title" in topic_info

    async def test_topic_not_found(self, http_server_url: str) -> None:
        """GET /topics/unknown/missing returns 404."""
        async with httpx.AsyncClient(base_url=http_server_url, timeout=TIMEOUT) as client:
            resp = await client.get("/topics/unknown/missing")

        assert resp.status_code == 404
        assert "error" in resp.json()
