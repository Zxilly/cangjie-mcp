"""FastMCP server for Cangjie documentation."""

from mcp.server.fastmcp import FastMCP

from cangjie_mcp.config import Settings
from cangjie_mcp.server import tools
from cangjie_mcp.server.tools import (
    CodeExample,
    SearchResult,
    ToolUsageResult,
    TopicResult,
)


def create_mcp_server(settings: Settings) -> FastMCP:
    """Create and configure the MCP server.

    Args:
        settings: Application settings

    Returns:
        Configured FastMCP instance
    """
    mcp = FastMCP(
        name="cangjie-docs",
        instructions="""
Cangjie Documentation Server - Provides access to Cangjie programming language documentation.

Available tools:
- search_docs: Semantic search across documentation
- get_topic: Get complete documentation for a specific topic
- list_topics: List available documentation topics
- get_code_examples: Get code examples for a feature
- get_tool_usage: Get usage information for tools like cjc, cjpm

The documentation language is configured at server startup.
        """.strip(),
    )

    # Create tool context
    ctx = tools.create_tool_context(settings)

    @mcp.tool()
    def search_docs(
        query: str,
        category: str | None = None,
        top_k: int = 5,
    ) -> list[SearchResult]:
        """Search Cangjie documentation using semantic search.

        Args:
            query: Search query describing what you're looking for
            category: Optional category to filter results (e.g., "cjpm", "syntax")
            top_k: Number of results to return (default: 5)

        Returns:
            List of matching documentation sections with content and metadata
        """
        return tools.search_docs(ctx, query=query, category=category, top_k=top_k)

    @mcp.tool()
    def get_topic(
        topic: str,
        category: str | None = None,
    ) -> TopicResult | str:
        """Get complete documentation for a specific topic.

        Args:
            topic: Topic name (the documentation file name without .md extension)
            category: Optional category to narrow the search

        Returns:
            Complete document content and metadata, or error message if not found
        """
        result = tools.get_topic(ctx, topic=topic, category=category)
        return result if result else f"Topic '{topic}' not found"

    @mcp.tool()
    def list_topics(category: str | None = None) -> dict[str, list[str]]:
        """List available documentation topics.

        Args:
            category: Optional category to filter by

        Returns:
            Dictionary mapping categories to lists of topic names
        """
        return tools.list_topics(ctx, category=category)

    @mcp.tool()
    def get_code_examples(feature: str, top_k: int = 3) -> list[CodeExample]:
        """Get code examples for a specific Cangjie feature.

        Args:
            feature: Feature to find examples for (e.g., "pattern matching", "async")
            top_k: Number of documents to search for examples (default: 3)

        Returns:
            List of code examples with language, code, and context
        """
        return tools.get_code_examples(ctx, feature=feature, top_k=top_k)

    @mcp.tool()
    def get_tool_usage(tool_name: str) -> ToolUsageResult | str:
        """Get usage information for Cangjie tools.

        Args:
            tool_name: Name of the tool (e.g., "cjc", "cjpm", "cjfmt")

        Returns:
            Tool usage information including commands and examples
        """
        result = tools.get_tool_usage(ctx, tool_name=tool_name)
        return result if result else f"No usage information found for tool '{tool_name}'"

    return mcp
