"""E2E integration tests for MCP tool calls via the JSON-RPC protocol.

Uses ``mcp.shared.memory`` to connect to the real ``FastMCP`` instance
through an in-memory transport.  Tool calls go through the full MCP
protocol path including lifespan initialization — catching bugs that
direct Python function calls miss.
"""

import asyncio
import json
from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from typing import Any

import anyio
import pytest
from mcp.client.session import ClientSession
from mcp.server.fastmcp import FastMCP
from mcp.shared.memory import create_client_server_memory_streams
from mcp.types import TextContent

from cangjie_mcp.config import IndexInfo, Settings
from cangjie_mcp.indexer.store import VectorStore
from cangjie_mcp.server.tools import (
    LifespanContext,
    ToolContext,
)
from tests.integration.conftest import TestDocumentSource, VectorStoreSearchIndex

# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


@pytest.fixture(scope="session")
def _mcp_tool_context(
    local_indexed_store: VectorStore,
    test_doc_source: TestDocumentSource,
    shared_local_settings: Settings,
) -> ToolContext:
    """Build a ToolContext from the shared session fixtures."""
    return ToolContext(
        settings=shared_local_settings,
        index_info=IndexInfo.from_settings(shared_local_settings),
        search_index=VectorStoreSearchIndex(local_indexed_store),
        document_source=test_doc_source,
    )


@asynccontextmanager
async def _mcp_session(tool_ctx: ToolContext):
    """Connect to the MCP server via in-memory transport.

    Must be called from within a single task (not across a yield in a
    pytest fixture) to avoid anyio cancel-scope issues.
    """
    from mcp.server.fastmcp.server import lifespan_wrapper

    from cangjie_mcp.server.tools import mcp as mcp_server

    low_level = mcp_server._mcp_server
    original_lifespan = low_level.lifespan

    @asynccontextmanager
    async def _test_lifespan(_app: FastMCP) -> AsyncIterator[LifespanContext]:
        ctx = LifespanContext()
        ctx.complete(tool_ctx)
        ctx.lsp_complete(False)
        yield ctx

    low_level.lifespan = lifespan_wrapper(mcp_server, _test_lifespan)

    try:
        async with create_client_server_memory_streams() as (
            client_streams,
            server_streams,
        ):
            client_read, client_write = client_streams
            server_read, server_write = server_streams

            server_task = asyncio.create_task(
                low_level.run(
                    server_read,
                    server_write,
                    low_level.create_initialization_options(),
                    raise_exceptions=False,
                )
            )

            async with ClientSession(
                read_stream=client_read,
                write_stream=client_write,
            ) as session:
                await session.initialize()
                yield session

            server_task.cancel()
            try:
                await server_task
            except (asyncio.CancelledError, anyio.get_cancelled_exc_class()):
                pass
    finally:
        low_level.lifespan = original_lifespan


def _get_text(result) -> str:
    """Extract the raw text string from a CallToolResult."""
    assert not result.isError, f"Tool call failed: {result.content}"
    content = result.content[0]
    assert isinstance(content, TextContent)
    return content.text


def _parse_tool_result(result) -> dict[str, Any]:
    """Extract the parsed JSON dict from a CallToolResult."""
    return json.loads(_get_text(result))


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


class TestMCPProtocolIntegration:
    """Test MCP tools via the full JSON-RPC protocol path."""

    async def test_search_docs_via_protocol(self, _mcp_tool_context) -> None:
        """cangjie_search_docs returns results through the protocol."""
        async with _mcp_session(_mcp_tool_context) as client:
            result = await client.call_tool(
                "cangjie_search_docs",
                {"query": "函数", "top_k": 3},
            )
        data = _parse_tool_result(result)

        assert data["count"] > 0
        assert len(data["items"]) == data["count"]
        for item in data["items"]:
            assert item["content"]
            assert item["score"] > 0
            assert item["category"]
            assert item["topic"]
            assert "has_code_examples" in item

    async def test_get_topic_via_protocol(self, _mcp_tool_context) -> None:
        """cangjie_get_topic returns full content through the protocol."""
        async with _mcp_session(_mcp_tool_context) as client:
            result = await client.call_tool(
                "cangjie_get_topic",
                {"topic": "functions", "category": "syntax"},
            )
        data = _parse_tool_result(result)

        assert data["content"]
        assert data["category"] == "syntax"
        assert data["topic"] == "functions"
        assert data["title"]
        assert "func" in data["content"].lower() or "函数" in data["content"]

    async def test_get_topic_not_found_via_protocol(self, _mcp_tool_context) -> None:
        """cangjie_get_topic returns an error string for missing topics."""
        async with _mcp_session(_mcp_tool_context) as client:
            result = await client.call_tool(
                "cangjie_get_topic",
                {"topic": "does_not_exist"},
            )
        text = _get_text(result)
        assert "not found" in text.lower()

    async def test_get_topic_not_found_suggests_similar(self, _mcp_tool_context) -> None:
        """cangjie_get_topic suggests similar topics when not found."""
        async with _mcp_session(_mcp_tool_context) as client:
            result = await client.call_tool(
                "cangjie_get_topic",
                {"topic": "function"},  # close to "functions"
            )
        text = _get_text(result)
        assert "not found" in text.lower()
        assert "did you mean" in text.lower()
        assert "functions" in text

    async def test_get_topic_not_found_invalid_category(self, _mcp_tool_context) -> None:
        """cangjie_get_topic reports invalid category with available categories."""
        async with _mcp_session(_mcp_tool_context) as client:
            result = await client.call_tool(
                "cangjie_get_topic",
                {"topic": "does_not_exist", "category": "nonexistent"},
            )
        text = _get_text(result)
        assert "not found" in text.lower()
        assert "nonexistent" in text

    async def test_list_topics_via_protocol(self, _mcp_tool_context) -> None:
        """cangjie_list_topics returns all categories with TopicInfo through the protocol."""
        async with _mcp_session(_mcp_tool_context) as client:
            result = await client.call_tool(
                "cangjie_list_topics",
                {},
            )
        data = _parse_tool_result(result)

        assert data["total_categories"] == 3
        assert data["total_topics"] == 6
        assert set(data["categories"].keys()) == {"basics", "syntax", "tools"}

        # Each topic should be a TopicInfo with name and title
        for _cat, topics in data["categories"].items():
            for topic_info in topics:
                assert "name" in topic_info
                assert "title" in topic_info

    async def test_list_topics_invalid_category(self, _mcp_tool_context) -> None:
        """cangjie_list_topics returns error for nonexistent category."""
        async with _mcp_session(_mcp_tool_context) as client:
            result = await client.call_tool(
                "cangjie_list_topics",
                {"category": "nonexistent"},
            )
        data = _parse_tool_result(result)

        assert data["error"] is not None
        assert "not found" in data["error"].lower()
        assert data["available_categories"] is not None
        assert len(data["available_categories"]) > 0
        assert data["total_categories"] == 0
        assert data["total_topics"] == 0
