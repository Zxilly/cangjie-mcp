"""Integration tests for MCP tool functions.

These tests verify the tool functions work correctly with
real document stores and indexed content.
"""

from pathlib import Path

from cangjie_mcp.config import Settings
from cangjie_mcp.indexer.document_source import PrebuiltDocumentSource
from cangjie_mcp.indexer.store import VectorStore
from cangjie_mcp.server import tools
from cangjie_mcp.server.tools import (
    GetCodeExamplesInput,
    GetToolUsageInput,
    GetTopicInput,
    ListTopicsInput,
    SearchDocsInput,
)


class TestToolsIntegration:
    """Integration tests for MCP tool functions."""

    def test_search_docs_tool(
        self,
        integration_docs_dir: Path,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """Test search_docs tool function."""
        document_source = PrebuiltDocumentSource(integration_docs_dir)
        ctx = tools.ToolContext(
            settings=shared_local_settings,
            store=local_indexed_store,
            document_source=document_source,
        )

        results = tools.search_docs(ctx, SearchDocsInput(query="变量声明", top_k=3))

        assert results.count > 0
        assert all(isinstance(r.content, str) for r in results.items)
        assert all(r.score > 0 for r in results.items)

    def test_get_topic_tool(
        self,
        integration_docs_dir: Path,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """Test get_topic tool function."""
        document_source = PrebuiltDocumentSource(integration_docs_dir)
        ctx = tools.ToolContext(
            settings=shared_local_settings,
            store=local_indexed_store,
            document_source=document_source,
        )

        result = tools.get_topic(ctx, GetTopicInput(topic="hello_world"))

        assert result is not None
        assert "Hello World" in result.content or "Hello, Cangjie" in result.content
        assert result.category == "basics"
        assert result.topic == "hello_world"

    def test_get_topic_not_found(
        self,
        integration_docs_dir: Path,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """Test get_topic returns None for non-existent topic."""
        document_source = PrebuiltDocumentSource(integration_docs_dir)
        ctx = tools.ToolContext(
            settings=shared_local_settings,
            store=local_indexed_store,
            document_source=document_source,
        )

        result = tools.get_topic(ctx, GetTopicInput(topic="nonexistent_topic"))
        assert result is None

    def test_list_topics_tool(
        self,
        integration_docs_dir: Path,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """Test list_topics tool function."""
        document_source = PrebuiltDocumentSource(integration_docs_dir)
        ctx = tools.ToolContext(
            settings=shared_local_settings,
            store=local_indexed_store,
            document_source=document_source,
        )

        result = tools.list_topics(ctx, ListTopicsInput())

        assert "basics" in result.categories
        assert "syntax" in result.categories
        assert "tools" in result.categories
        assert "hello_world" in result.categories["basics"]
        assert "functions" in result.categories["syntax"]

    def test_list_topics_by_category(
        self,
        integration_docs_dir: Path,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """Test list_topics with category filter."""
        document_source = PrebuiltDocumentSource(integration_docs_dir)
        ctx = tools.ToolContext(
            settings=shared_local_settings,
            store=local_indexed_store,
            document_source=document_source,
        )

        result = tools.list_topics(ctx, ListTopicsInput(category="tools"))

        assert result.total_categories == 1
        assert "tools" in result.categories
        assert "cjc" in result.categories["tools"]
        assert "cjpm" in result.categories["tools"]

    def test_get_code_examples_tool(
        self,
        integration_docs_dir: Path,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """Test get_code_examples tool function."""
        document_source = PrebuiltDocumentSource(integration_docs_dir)
        ctx = tools.ToolContext(
            settings=shared_local_settings,
            store=local_indexed_store,
            document_source=document_source,
        )

        examples = tools.get_code_examples(ctx, GetCodeExamplesInput(feature="函数", top_k=3))

        assert len(examples) > 0
        assert all(isinstance(e.language, str) for e in examples)
        assert all(isinstance(e.code, str) for e in examples)

    def test_get_tool_usage_tool(
        self,
        integration_docs_dir: Path,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """Test get_tool_usage tool function."""
        document_source = PrebuiltDocumentSource(integration_docs_dir)
        ctx = tools.ToolContext(
            settings=shared_local_settings,
            store=local_indexed_store,
            document_source=document_source,
        )

        result = tools.get_tool_usage(ctx, GetToolUsageInput(tool_name="cjpm"))

        assert result is not None
        assert result.tool_name == "cjpm"
        assert "cjpm" in result.content.lower()
        assert isinstance(result.examples, list)

    def test_search_with_category_filter(
        self,
        integration_docs_dir: Path,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """Test search_docs with category filter."""
        document_source = PrebuiltDocumentSource(integration_docs_dir)
        ctx = tools.ToolContext(
            settings=shared_local_settings,
            store=local_indexed_store,
            document_source=document_source,
        )

        results = tools.search_docs(ctx, SearchDocsInput(query="编译", category="tools", top_k=3))

        assert results.count > 0
        assert all(r.category == "tools" for r in results.items)

    def test_get_topic_with_category(
        self,
        integration_docs_dir: Path,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """Test get_topic with explicit category."""
        document_source = PrebuiltDocumentSource(integration_docs_dir)
        ctx = tools.ToolContext(
            settings=shared_local_settings,
            store=local_indexed_store,
            document_source=document_source,
        )

        result = tools.get_topic(ctx, GetTopicInput(topic="cjc", category="tools"))

        assert result is not None
        assert result.category == "tools"
        assert "cjc" in result.content.lower() or "编译" in result.content

    def test_code_examples_filter_by_language(
        self,
        integration_docs_dir: Path,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """Test get_code_examples returns examples with expected languages."""
        document_source = PrebuiltDocumentSource(integration_docs_dir)
        ctx = tools.ToolContext(
            settings=shared_local_settings,
            store=local_indexed_store,
            document_source=document_source,
        )

        examples = tools.get_code_examples(ctx, GetCodeExamplesInput(feature="编译", top_k=5))

        languages = {e.language for e in examples}
        # Should have bash or cangjie examples
        assert len(languages) > 0
        assert any(lang in languages for lang in ["bash", "cangjie"])
