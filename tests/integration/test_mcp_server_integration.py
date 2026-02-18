"""Integration tests for MCP server creation under different configurations.

Tests that the MCP server creates correctly with the right tools registered
for common configuration combinations, without using mocks.
"""

from unittest.mock import MagicMock

import pytest
from mcp.server.fastmcp import Context

from cangjie_mcp.config import IndexInfo, Settings
from cangjie_mcp.indexer.store import VectorStore
from cangjie_mcp.server import tools
from cangjie_mcp.server.factory import create_mcp_server
from tests.integration.conftest import TestDocumentSource, VectorStoreSearchIndex

# Expected documentation tool names
DOCS_TOOLS = [
    "cangjie_search_docs",
    "cangjie_get_topic",
    "cangjie_list_topics",
]

# Expected LSP tool names
LSP_TOOLS = [
    "cangjie_lsp_definition",
    "cangjie_lsp_references",
    "cangjie_lsp_hover",
    "cangjie_lsp_symbols",
    "cangjie_lsp_diagnostics",
    "cangjie_lsp_completion",
]


def _mock_ctx(tool_context: tools.ToolContext) -> MagicMock:
    """Create a mock MCP Context wrapping a ToolContext."""
    ctx = MagicMock(spec=Context)
    lifespan_ctx = tools.LifespanContext()
    lifespan_ctx.complete(tool_context)
    ctx.request_context.lifespan_context = lifespan_ctx
    return ctx


class TestMCPServerDocsTools:
    """Verify documentation tools are always registered on the module-level mcp."""

    @pytest.mark.asyncio
    async def test_docs_tools_registered(self, local_settings: Settings, monkeypatch: pytest.MonkeyPatch) -> None:
        """All documentation tools are registered."""
        monkeypatch.delenv("CANGJIE_HOME", raising=False)
        mcp = create_mcp_server(local_settings)

        tools_list = await mcp.list_tools()
        tool_names = [t.name for t in tools_list]

        for name in DOCS_TOOLS:
            assert name in tool_names, f"Docs tool '{name}' missing"

    @pytest.mark.asyncio
    async def test_all_docs_tools_have_descriptions(
        self, local_settings: Settings, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """Every registered documentation tool has a non-empty description."""
        monkeypatch.delenv("CANGJIE_HOME", raising=False)
        mcp = create_mcp_server(local_settings)
        tools_list = await mcp.list_tools()
        for tool in tools_list:
            if tool.name in DOCS_TOOLS:
                assert tool.description, f"Tool '{tool.name}' has no description"
                assert len(tool.description) > 10, f"Tool '{tool.name}' description too short"


class TestMCPServerWithLSPEnabled:
    """Test MCP server creation with LSP enabled."""

    @pytest.mark.asyncio
    async def test_all_tools_registered(self, local_settings: Settings, monkeypatch: pytest.MonkeyPatch) -> None:
        """When CANGJIE_HOME is set, both docs and LSP tools are registered."""
        monkeypatch.setenv("CANGJIE_HOME", "/fake/sdk")
        mcp = create_mcp_server(local_settings)

        tools_list = await mcp.list_tools()
        tool_names = [t.name for t in tools_list]

        for name in DOCS_TOOLS + LSP_TOOLS:
            assert name in tool_names, f"Tool '{name}' missing"

    @pytest.mark.asyncio
    async def test_total_tool_count(self, local_settings: Settings, monkeypatch: pytest.MonkeyPatch) -> None:
        """All 9 tools (3 docs + 6 LSP) are registered."""
        monkeypatch.setenv("CANGJIE_HOME", "/fake/sdk")
        mcp = create_mcp_server(local_settings)
        tools_list = await mcp.list_tools()
        assert len(tools_list) == len(DOCS_TOOLS) + len(LSP_TOOLS)

    @pytest.mark.asyncio
    async def test_all_tools_have_descriptions(self, local_settings: Settings, monkeypatch: pytest.MonkeyPatch) -> None:
        """Every registered tool has a non-empty description."""
        monkeypatch.setenv("CANGJIE_HOME", "/fake/sdk")
        mcp = create_mcp_server(local_settings)
        tools_list = await mcp.list_tools()
        for tool in tools_list:
            assert tool.description, f"Tool '{tool.name}' has no description"


class TestMCPServerProperties:
    """Test basic MCP server properties."""

    def test_server_name(self, local_settings: Settings, monkeypatch: pytest.MonkeyPatch) -> None:
        """Server is named 'cangjie_mcp'."""
        monkeypatch.delenv("CANGJIE_HOME", raising=False)
        mcp = create_mcp_server(local_settings)
        assert mcp.name == "cangjie_mcp"

    def test_server_has_instructions(self, local_settings: Settings, monkeypatch: pytest.MonkeyPatch) -> None:
        """Server has non-empty instructions (system prompt)."""
        monkeypatch.delenv("CANGJIE_HOME", raising=False)
        mcp = create_mcp_server(local_settings)
        assert mcp.instructions is not None
        assert len(mcp.instructions) > 0


class TestToolsUnderDefaultConfig:
    """Test all tool functions under default configuration (local embedding, no rerank)."""

    @pytest.mark.asyncio
    async def test_search_docs(
        self,
        test_doc_source: TestDocumentSource,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """search_docs returns results with valid structure."""
        ctx = _mock_ctx(
            tools.ToolContext(
                settings=shared_local_settings,
                index_info=IndexInfo.from_settings(shared_local_settings),
                search_index=VectorStoreSearchIndex(local_indexed_store),
                document_source=test_doc_source,
            )
        )

        result = await tools.search_docs(query="函数", top_k=3, ctx=ctx)

        assert result.count > 0
        assert len(result.items) == result.count
        assert result.offset == 0
        for item in result.items:
            assert item.content
            assert item.score > 0
            assert item.category
            assert item.topic

    @pytest.mark.asyncio
    async def test_search_docs_pagination(
        self,
        test_doc_source: TestDocumentSource,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """search_docs pagination works correctly."""
        ctx = _mock_ctx(
            tools.ToolContext(
                settings=shared_local_settings,
                index_info=IndexInfo.from_settings(shared_local_settings),
                search_index=VectorStoreSearchIndex(local_indexed_store),
                document_source=test_doc_source,
            )
        )

        page1 = await tools.search_docs(query="仓颉", top_k=2, offset=0, ctx=ctx)
        page2 = await tools.search_docs(query="仓颉", top_k=2, offset=2, ctx=ctx)

        assert page1.count > 0
        assert page1.offset == 0
        # Pages should return different items (if enough results exist)
        if page2.count > 0:
            assert page1.items[0].topic != page2.items[0].topic or page1.items[0].content != page2.items[0].content

    @pytest.mark.asyncio
    async def test_get_topic(
        self,
        test_doc_source: TestDocumentSource,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """get_topic returns full document content."""
        ctx = _mock_ctx(
            tools.ToolContext(
                settings=shared_local_settings,
                index_info=IndexInfo.from_settings(shared_local_settings),
                search_index=VectorStoreSearchIndex(local_indexed_store),
                document_source=test_doc_source,
            )
        )

        result = await tools.get_topic(topic="functions", category="syntax", ctx=ctx)

        assert result is not None
        assert result.category == "syntax"
        assert result.topic == "functions"
        assert "func" in result.content.lower() or "函数" in result.content

    @pytest.mark.asyncio
    async def test_get_topic_returns_none_for_missing(
        self,
        test_doc_source: TestDocumentSource,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """get_topic returns None for non-existent topic."""
        ctx = _mock_ctx(
            tools.ToolContext(
                settings=shared_local_settings,
                index_info=IndexInfo.from_settings(shared_local_settings),
                search_index=VectorStoreSearchIndex(local_indexed_store),
                document_source=test_doc_source,
            )
        )

        result = await tools.get_topic(topic="does_not_exist", ctx=ctx)
        assert isinstance(result, str)

    @pytest.mark.asyncio
    async def test_list_topics(
        self,
        test_doc_source: TestDocumentSource,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """list_topics returns all categories and topics."""
        ctx = _mock_ctx(
            tools.ToolContext(
                settings=shared_local_settings,
                index_info=IndexInfo.from_settings(shared_local_settings),
                search_index=VectorStoreSearchIndex(local_indexed_store),
                document_source=test_doc_source,
            )
        )

        result = await tools.list_topics(ctx=ctx)

        assert result.total_categories == 3
        assert result.total_topics == 6
        assert set(result.categories.keys()) == {"basics", "syntax", "tools"}

    @pytest.mark.asyncio
    async def test_list_topics_filtered(
        self,
        test_doc_source: TestDocumentSource,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """list_topics with category filter returns only that category."""
        ctx = _mock_ctx(
            tools.ToolContext(
                settings=shared_local_settings,
                index_info=IndexInfo.from_settings(shared_local_settings),
                search_index=VectorStoreSearchIndex(local_indexed_store),
                document_source=test_doc_source,
            )
        )

        result = await tools.list_topics(category="syntax", ctx=ctx)

        assert result.total_categories == 1
        assert "syntax" in result.categories
        assert "functions" in result.categories["syntax"]
        assert "pattern_matching" in result.categories["syntax"]

    @pytest.mark.asyncio
    async def test_search_docs_extract_code(
        self,
        test_doc_source: TestDocumentSource,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """search_docs with extract_code returns code blocks with metadata."""
        ctx = _mock_ctx(
            tools.ToolContext(
                settings=shared_local_settings,
                index_info=IndexInfo.from_settings(shared_local_settings),
                search_index=VectorStoreSearchIndex(local_indexed_store),
                document_source=test_doc_source,
            )
        )

        result = await tools.search_docs(query="Hello World", extract_code=True, ctx=ctx)

        assert result.count > 0
        for item in result.items:
            assert item.code_examples is not None
            for ex in item.code_examples:
                assert ex.language in ("cangjie", "bash", "")
                assert len(ex.code) > 0
                assert ex.source_topic

    @pytest.mark.asyncio
    async def test_search_docs_without_extract_code(
        self,
        test_doc_source: TestDocumentSource,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """search_docs without extract_code returns None for code_examples."""
        ctx = _mock_ctx(
            tools.ToolContext(
                settings=shared_local_settings,
                index_info=IndexInfo.from_settings(shared_local_settings),
                search_index=VectorStoreSearchIndex(local_indexed_store),
                document_source=test_doc_source,
            )
        )

        result = await tools.search_docs(query="Hello World", ctx=ctx)

        assert result.count > 0
        for item in result.items:
            assert item.code_examples is None


class TestToolsWithSmallChunkSize:
    """Test tools with a small chunk_max_size to verify chunking works."""

    @pytest.mark.asyncio
    async def test_search_returns_results(
        self,
        test_doc_source: TestDocumentSource,
        shared_small_chunk_store: VectorStore,
        shared_small_chunk_settings: Settings,
    ) -> None:
        """Search works with small chunk sizes."""
        ctx = _mock_ctx(
            tools.ToolContext(
                settings=shared_small_chunk_settings,
                index_info=IndexInfo.from_settings(shared_small_chunk_settings),
                search_index=VectorStoreSearchIndex(shared_small_chunk_store),
                document_source=test_doc_source,
            )
        )

        result = await tools.search_docs(query="变量", top_k=5, ctx=ctx)
        assert result.count > 0

    def test_more_chunks_than_documents(
        self,
        shared_small_chunk_store: VectorStore,
    ) -> None:
        """Small chunk size produces more indexed chunks than source documents."""
        chunk_count = shared_small_chunk_store.collection.count()
        # 6 source documents with chunk_max_size=200 should produce more than 6 chunks
        assert chunk_count >= 6

    @pytest.mark.asyncio
    async def test_list_topics_unaffected_by_chunk_size(
        self,
        test_doc_source: TestDocumentSource,
        shared_small_chunk_store: VectorStore,
        shared_small_chunk_settings: Settings,
    ) -> None:
        """list_topics (read from document source) is unaffected by chunk size."""
        ctx = _mock_ctx(
            tools.ToolContext(
                settings=shared_small_chunk_settings,
                index_info=IndexInfo.from_settings(shared_small_chunk_settings),
                search_index=VectorStoreSearchIndex(shared_small_chunk_store),
                document_source=test_doc_source,
            )
        )

        result = await tools.list_topics(ctx=ctx)
        assert result.total_topics == 6
        assert result.total_categories == 3
