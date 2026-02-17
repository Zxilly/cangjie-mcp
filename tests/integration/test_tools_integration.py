"""Integration tests for MCP tool functions.

These tests verify the tool functions work correctly with
real document stores and indexed content.
"""

from unittest.mock import MagicMock

import pytest
from mcp.server.fastmcp import Context

from cangjie_mcp.config import IndexInfo, Settings
from cangjie_mcp.indexer.store import VectorStore
from cangjie_mcp.server import tools
from tests.integration.conftest import TestDocumentSource, VectorStoreSearchIndex


def _mock_ctx(tool_context: tools.ToolContext) -> MagicMock:
    """Create a mock MCP Context wrapping a ToolContext."""
    ctx = MagicMock(spec=Context)
    lifespan_ctx = tools.LifespanContext()
    lifespan_ctx.complete(tool_context)
    ctx.request_context.lifespan_context = lifespan_ctx
    return ctx


class TestToolsIntegration:
    """Integration tests for MCP tool functions."""

    @pytest.mark.asyncio
    async def test_search_docs_tool(
        self,
        test_doc_source: TestDocumentSource,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """Test search_docs tool function."""
        ctx = _mock_ctx(
            tools.ToolContext(
                settings=shared_local_settings,
                index_info=IndexInfo.from_settings(shared_local_settings),
                search_index=VectorStoreSearchIndex(local_indexed_store),
                document_source=test_doc_source,
            )
        )

        results = await tools.search_docs(query="变量声明", top_k=3, ctx=ctx)

        assert results.count > 0
        assert all(isinstance(r.content, str) for r in results.items)
        assert all(r.score > 0 for r in results.items)

    @pytest.mark.asyncio
    async def test_get_topic_tool(
        self,
        test_doc_source: TestDocumentSource,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """Test get_topic tool function."""
        ctx = _mock_ctx(
            tools.ToolContext(
                settings=shared_local_settings,
                index_info=IndexInfo.from_settings(shared_local_settings),
                search_index=VectorStoreSearchIndex(local_indexed_store),
                document_source=test_doc_source,
            )
        )

        result = await tools.get_topic(topic="hello_world", ctx=ctx)

        assert result is not None
        assert "Hello World" in result.content or "Hello, Cangjie" in result.content
        assert result.category == "basics"
        assert result.topic == "hello_world"

    @pytest.mark.asyncio
    async def test_get_topic_not_found(
        self,
        test_doc_source: TestDocumentSource,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """Test get_topic returns None for non-existent topic."""
        ctx = _mock_ctx(
            tools.ToolContext(
                settings=shared_local_settings,
                index_info=IndexInfo.from_settings(shared_local_settings),
                search_index=VectorStoreSearchIndex(local_indexed_store),
                document_source=test_doc_source,
            )
        )

        result = await tools.get_topic(topic="nonexistent_topic", ctx=ctx)
        assert isinstance(result, str)

    @pytest.mark.asyncio
    async def test_list_topics_tool(
        self,
        test_doc_source: TestDocumentSource,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """Test list_topics tool function."""
        ctx = _mock_ctx(
            tools.ToolContext(
                settings=shared_local_settings,
                index_info=IndexInfo.from_settings(shared_local_settings),
                search_index=VectorStoreSearchIndex(local_indexed_store),
                document_source=test_doc_source,
            )
        )

        result = await tools.list_topics(ctx=ctx)

        assert "basics" in result.categories
        assert "syntax" in result.categories
        assert "tools" in result.categories
        assert "hello_world" in result.categories["basics"]
        assert "functions" in result.categories["syntax"]

    @pytest.mark.asyncio
    async def test_list_topics_by_category(
        self,
        test_doc_source: TestDocumentSource,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """Test list_topics with category filter."""
        ctx = _mock_ctx(
            tools.ToolContext(
                settings=shared_local_settings,
                index_info=IndexInfo.from_settings(shared_local_settings),
                search_index=VectorStoreSearchIndex(local_indexed_store),
                document_source=test_doc_source,
            )
        )

        result = await tools.list_topics(category="tools", ctx=ctx)

        assert result.total_categories == 1
        assert "tools" in result.categories
        assert "cjc" in result.categories["tools"]
        assert "cjpm" in result.categories["tools"]

    @pytest.mark.asyncio
    async def test_get_code_examples_tool(
        self,
        test_doc_source: TestDocumentSource,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """Test get_code_examples tool function."""
        ctx = _mock_ctx(
            tools.ToolContext(
                settings=shared_local_settings,
                index_info=IndexInfo.from_settings(shared_local_settings),
                search_index=VectorStoreSearchIndex(local_indexed_store),
                document_source=test_doc_source,
            )
        )

        examples = await tools.get_code_examples(feature="函数", top_k=3, ctx=ctx)

        assert len(examples) > 0
        assert all(isinstance(e.language, str) for e in examples)
        assert all(isinstance(e.code, str) for e in examples)

    @pytest.mark.asyncio
    async def test_get_tool_usage_tool(
        self,
        test_doc_source: TestDocumentSource,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """Test get_tool_usage tool function."""
        ctx = _mock_ctx(
            tools.ToolContext(
                settings=shared_local_settings,
                index_info=IndexInfo.from_settings(shared_local_settings),
                search_index=VectorStoreSearchIndex(local_indexed_store),
                document_source=test_doc_source,
            )
        )

        result = await tools.get_tool_usage(tool_name="cjpm", ctx=ctx)

        assert result is not None
        assert result.tool_name == "cjpm"
        assert "cjpm" in result.content.lower()
        assert isinstance(result.examples, list)

    @pytest.mark.asyncio
    async def test_search_with_category_filter(
        self,
        test_doc_source: TestDocumentSource,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """Test search_docs with category filter."""
        ctx = _mock_ctx(
            tools.ToolContext(
                settings=shared_local_settings,
                index_info=IndexInfo.from_settings(shared_local_settings),
                search_index=VectorStoreSearchIndex(local_indexed_store),
                document_source=test_doc_source,
            )
        )

        results = await tools.search_docs(query="编译", category="tools", top_k=3, ctx=ctx)

        assert results.count > 0
        assert all(r.category == "tools" for r in results.items)

    @pytest.mark.asyncio
    async def test_get_topic_with_category(
        self,
        test_doc_source: TestDocumentSource,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """Test get_topic with explicit category."""
        ctx = _mock_ctx(
            tools.ToolContext(
                settings=shared_local_settings,
                index_info=IndexInfo.from_settings(shared_local_settings),
                search_index=VectorStoreSearchIndex(local_indexed_store),
                document_source=test_doc_source,
            )
        )

        result = await tools.get_topic(topic="cjc", category="tools", ctx=ctx)

        assert result is not None
        assert result.category == "tools"
        assert "cjc" in result.content.lower() or "编译" in result.content

    @pytest.mark.asyncio
    async def test_code_examples_filter_by_language(
        self,
        test_doc_source: TestDocumentSource,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """Test get_code_examples returns examples with expected languages."""
        ctx = _mock_ctx(
            tools.ToolContext(
                settings=shared_local_settings,
                index_info=IndexInfo.from_settings(shared_local_settings),
                search_index=VectorStoreSearchIndex(local_indexed_store),
                document_source=test_doc_source,
            )
        )

        examples = await tools.get_code_examples(feature="编译", top_k=5, ctx=ctx)

        languages = {e.language for e in examples}
        # Should have bash or cangjie examples
        assert len(languages) > 0
        assert any(lang in languages for lang in ["bash", "cangjie"])
