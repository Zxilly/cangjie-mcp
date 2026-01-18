"""Integration tests for MCP tool functions.

These tests verify the tool functions work correctly with
real document stores and indexed content.
"""

from pathlib import Path

from cangjie_mcp.config import Settings
from cangjie_mcp.indexer.loader import DocumentLoader
from cangjie_mcp.indexer.store import VectorStore
from cangjie_mcp.server import tools


class TestToolsIntegration:
    """Integration tests for MCP tool functions."""

    def test_search_docs_tool(
        self,
        integration_docs_dir: Path,
        local_indexed_store: VectorStore,
        local_settings: Settings,
    ) -> None:
        """Test search_docs tool function."""
        loader = DocumentLoader(integration_docs_dir)
        ctx = tools.ToolContext(
            settings=local_settings,
            store=local_indexed_store,
            loader=loader,
        )

        results = tools.search_docs(ctx, query="变量声明", top_k=3)

        assert len(results) > 0
        assert all(isinstance(r, dict) for r in results)
        assert all("content" in r and "score" in r for r in results)

    def test_get_topic_tool(
        self,
        integration_docs_dir: Path,
        local_indexed_store: VectorStore,
        local_settings: Settings,
    ) -> None:
        """Test get_topic tool function."""
        loader = DocumentLoader(integration_docs_dir)
        ctx = tools.ToolContext(
            settings=local_settings,
            store=local_indexed_store,
            loader=loader,
        )

        result = tools.get_topic(ctx, topic="hello_world")

        assert result is not None
        assert "Hello World" in result["content"] or "Hello, Cangjie" in result["content"]
        assert result["category"] == "basics"
        assert result["topic"] == "hello_world"

    def test_get_topic_not_found(
        self,
        integration_docs_dir: Path,
        local_indexed_store: VectorStore,
        local_settings: Settings,
    ) -> None:
        """Test get_topic returns None for non-existent topic."""
        loader = DocumentLoader(integration_docs_dir)
        ctx = tools.ToolContext(
            settings=local_settings,
            store=local_indexed_store,
            loader=loader,
        )

        result = tools.get_topic(ctx, topic="nonexistent_topic")
        assert result is None

    def test_list_topics_tool(
        self,
        integration_docs_dir: Path,
        local_indexed_store: VectorStore,
        local_settings: Settings,
    ) -> None:
        """Test list_topics tool function."""
        loader = DocumentLoader(integration_docs_dir)
        ctx = tools.ToolContext(
            settings=local_settings,
            store=local_indexed_store,
            loader=loader,
        )

        all_topics = tools.list_topics(ctx)

        assert "basics" in all_topics
        assert "syntax" in all_topics
        assert "tools" in all_topics
        assert "hello_world" in all_topics["basics"]
        assert "functions" in all_topics["syntax"]

    def test_list_topics_by_category(
        self,
        integration_docs_dir: Path,
        local_indexed_store: VectorStore,
        local_settings: Settings,
    ) -> None:
        """Test list_topics with category filter."""
        loader = DocumentLoader(integration_docs_dir)
        ctx = tools.ToolContext(
            settings=local_settings,
            store=local_indexed_store,
            loader=loader,
        )

        topics = tools.list_topics(ctx, category="tools")

        assert len(topics) == 1
        assert "tools" in topics
        assert "cjc" in topics["tools"]
        assert "cjpm" in topics["tools"]

    def test_get_code_examples_tool(
        self,
        integration_docs_dir: Path,
        local_indexed_store: VectorStore,
        local_settings: Settings,
    ) -> None:
        """Test get_code_examples tool function."""
        loader = DocumentLoader(integration_docs_dir)
        ctx = tools.ToolContext(
            settings=local_settings,
            store=local_indexed_store,
            loader=loader,
        )

        examples = tools.get_code_examples(ctx, feature="函数", top_k=3)

        assert len(examples) > 0
        assert all(isinstance(e, dict) for e in examples)
        assert all("language" in e and "code" in e for e in examples)

    def test_get_tool_usage_tool(
        self,
        integration_docs_dir: Path,
        local_indexed_store: VectorStore,
        local_settings: Settings,
    ) -> None:
        """Test get_tool_usage tool function."""
        loader = DocumentLoader(integration_docs_dir)
        ctx = tools.ToolContext(
            settings=local_settings,
            store=local_indexed_store,
            loader=loader,
        )

        result = tools.get_tool_usage(ctx, tool_name="cjpm")

        assert result is not None
        assert result["tool_name"] == "cjpm"
        assert "cjpm" in result["content"].lower()
        assert isinstance(result["examples"], list)

    def test_search_with_category_filter(
        self,
        integration_docs_dir: Path,
        local_indexed_store: VectorStore,
        local_settings: Settings,
    ) -> None:
        """Test search_docs with category filter."""
        loader = DocumentLoader(integration_docs_dir)
        ctx = tools.ToolContext(
            settings=local_settings,
            store=local_indexed_store,
            loader=loader,
        )

        results = tools.search_docs(ctx, query="编译", category="tools", top_k=3)

        assert len(results) > 0
        assert all(r["category"] == "tools" for r in results)

    def test_get_topic_with_category(
        self,
        integration_docs_dir: Path,
        local_indexed_store: VectorStore,
        local_settings: Settings,
    ) -> None:
        """Test get_topic with explicit category."""
        loader = DocumentLoader(integration_docs_dir)
        ctx = tools.ToolContext(
            settings=local_settings,
            store=local_indexed_store,
            loader=loader,
        )

        result = tools.get_topic(ctx, topic="cjc", category="tools")

        assert result is not None
        assert result["category"] == "tools"
        assert "cjc" in result["content"].lower() or "编译" in result["content"]

    def test_code_examples_filter_by_language(
        self,
        integration_docs_dir: Path,
        local_indexed_store: VectorStore,
        local_settings: Settings,
    ) -> None:
        """Test get_code_examples returns examples with expected languages."""
        loader = DocumentLoader(integration_docs_dir)
        ctx = tools.ToolContext(
            settings=local_settings,
            store=local_indexed_store,
            loader=loader,
        )

        examples = tools.get_code_examples(ctx, feature="编译", top_k=5)

        languages = {e["language"] for e in examples}
        # Should have bash or cangjie examples
        assert len(languages) > 0
        assert any(lang in languages for lang in ["bash", "cangjie"])
