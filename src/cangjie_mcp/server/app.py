"""FastMCP server for Cangjie documentation."""

from __future__ import annotations

from typing import TYPE_CHECKING

from mcp.server.fastmcp import FastMCP
from mcp.types import ToolAnnotations

from cangjie_mcp.config import IndexKey, Settings
from cangjie_mcp.server import tools
from cangjie_mcp.server.tools import (
    CodeExample,
    GetCodeExamplesInput,
    GetToolUsageInput,
    GetTopicInput,
    ListTopicsInput,
    SearchDocsInput,
    SearchResult,
    ToolContext,
    ToolUsageResult,
    TopicResult,
    TopicsListResult,
)

if TYPE_CHECKING:
    from cangjie_mcp.indexer.store import VectorStore


# =============================================================================
# Server Instructions
# =============================================================================

SERVER_INSTRUCTIONS = """
Cangjie Documentation Server - Provides semantic search and retrieval of
Cangjie programming language documentation.

Available tools:
- cangjie_search_docs: Semantic search across documentation with pagination
- cangjie_get_topic: Get complete documentation for a specific topic
- cangjie_list_topics: List available documentation topics by category
- cangjie_get_code_examples: Get code examples for a language feature
- cangjie_get_tool_usage: Get usage information for Cangjie CLI tools

Workflow recommendations:
1. Use cangjie_list_topics to discover available documentation categories and topics
2. Use cangjie_search_docs for semantic queries about specific concepts
3. Use cangjie_get_topic to retrieve full documentation for a known topic
4. Use cangjie_get_code_examples to find practical code examples
5. Use cangjie_get_tool_usage for CLI tool documentation (cjc, cjpm, etc.)
""".strip()


# =============================================================================
# Tool Registration
# =============================================================================


def _register_tools(mcp: FastMCP, ctx: ToolContext) -> None:
    """Register all MCP tools with the server.

    This function is shared between create_mcp_server and create_mcp_server_with_store
    to avoid code duplication.

    Args:
        mcp: FastMCP server instance
        ctx: Tool context with store and loader
    """

    @mcp.tool(
        name="cangjie_search_docs",
        annotations=ToolAnnotations(
            title="Search Cangjie Documentation",
            readOnlyHint=True,
            destructiveHint=False,
            idempotentHint=True,
            openWorldHint=False,
        ),
    )
    def cangjie_search_docs(params: SearchDocsInput) -> SearchResult:
        """Search Cangjie documentation using semantic search.

        Performs vector similarity search across all indexed documentation.
        Returns matching sections ranked by relevance with pagination support.

        Args:
            params: Search parameters including:
                - query (str): Search query describing what you're looking for
                - category (str | None): Optional category filter (e.g., 'cjpm', 'syntax')
                - top_k (int): Number of results to return (default: 5, max: 20)
                - offset (int): Pagination offset (default: 0)

        Returns:
            SearchResult containing:
                - items: List of matching documents with content, score, and metadata
                - total: Estimated total matches
                - count: Number of items in this response
                - offset: Current pagination offset
                - has_more: Whether more results are available
                - next_offset: Next offset for pagination (or None)

        Examples:
            - Query: "how to define a class" -> Returns class definition docs
            - Query: "pattern matching syntax" -> Returns pattern matching docs
            - Query: "async programming" with category="stdlib" -> Filters to stdlib
        """
        return tools.search_docs(ctx, params)

    @mcp.tool(
        name="cangjie_get_topic",
        annotations=ToolAnnotations(
            title="Get Documentation Topic",
            readOnlyHint=True,
            destructiveHint=False,
            idempotentHint=True,
            openWorldHint=False,
        ),
    )
    def cangjie_get_topic(params: GetTopicInput) -> TopicResult | str:
        """Get complete documentation for a specific topic.

        Retrieves the full content of a documentation file by topic name.
        Use cangjie_list_topics first to discover available topic names.

        Args:
            params: Input parameters including:
                - topic (str): Topic name (file name without .md extension)
                - category (str | None): Optional category to narrow search

        Returns:
            TopicResult with full document content and metadata, or error string if not found.
            TopicResult contains:
                - content: Full markdown content of the document
                - file_path: Path to the source file
                - category: Document category
                - topic: Topic name
                - title: Document title

        Examples:
            - topic="classes" -> Returns full class documentation
            - topic="pattern-matching", category="syntax" -> Specific category lookup
        """
        result = tools.get_topic(ctx, params)
        return result if result else f"Topic '{params.topic}' not found"

    @mcp.tool(
        name="cangjie_list_topics",
        annotations=ToolAnnotations(
            title="List Documentation Topics",
            readOnlyHint=True,
            destructiveHint=False,
            idempotentHint=True,
            openWorldHint=False,
        ),
    )
    def cangjie_list_topics(params: ListTopicsInput) -> TopicsListResult:
        """List available documentation topics organized by category.

        Returns all documentation topics, optionally filtered by category.
        Use this to discover topic names for use with cangjie_get_topic.

        Args:
            params: Input parameters including:
                - category (str | None): Optional category filter

        Returns:
            TopicsListResult containing:
                - categories: Dict mapping category names to lists of topic names
                - total_categories: Number of categories
                - total_topics: Total number of topics across all categories

        Examples:
            - No params -> Returns all categories and their topics
            - category="cjpm" -> Returns only cjpm-related topics
        """
        return tools.list_topics(ctx, params)

    @mcp.tool(
        name="cangjie_get_code_examples",
        annotations=ToolAnnotations(
            title="Get Code Examples",
            readOnlyHint=True,
            destructiveHint=False,
            idempotentHint=True,
            openWorldHint=False,
        ),
    )
    def cangjie_get_code_examples(params: GetCodeExamplesInput) -> list[CodeExample]:
        """Get code examples for a specific Cangjie language feature.

        Searches documentation for code blocks related to a feature.
        Returns extracted code examples with their surrounding context.

        Args:
            params: Input parameters including:
                - feature (str): Feature to find examples for
                - top_k (int): Number of documents to search (default: 3)

        Returns:
            List of CodeExample objects, each containing:
                - language: Programming language of the code block
                - code: The actual code content
                - context: Surrounding text providing context
                - source_topic: Topic where the example was found
                - source_file: Source file path

        Examples:
            - feature="pattern matching" -> Pattern matching code examples
            - feature="generics" -> Generic type usage examples
            - feature="async/await" -> Async programming examples
        """
        return tools.get_code_examples(ctx, params)

    @mcp.tool(
        name="cangjie_get_tool_usage",
        annotations=ToolAnnotations(
            title="Get Tool Usage",
            readOnlyHint=True,
            destructiveHint=False,
            idempotentHint=True,
            openWorldHint=False,
        ),
    )
    def cangjie_get_tool_usage(params: GetToolUsageInput) -> ToolUsageResult | str:
        """Get usage information for Cangjie development tools.

        Searches for documentation about Cangjie CLI tools including
        compiler, package manager, formatter, and other utilities.

        Args:
            params: Input parameters including:
                - tool_name (str): Name of the tool (e.g., 'cjc', 'cjpm', 'cjfmt')

        Returns:
            ToolUsageResult with documentation and examples, or error string if not found.
            ToolUsageResult contains:
                - tool_name: Name of the tool
                - content: Combined documentation content
                - examples: List of shell command examples with context

        Examples:
            - tool_name="cjc" -> Compiler usage and options
            - tool_name="cjpm" -> Package manager commands
            - tool_name="cjfmt" -> Code formatter usage
        """
        result = tools.get_tool_usage(ctx, params)
        return result if result else f"No usage information found for tool '{params.tool_name}'"


# =============================================================================
# Server Factory Functions
# =============================================================================


def create_mcp_server(settings: Settings) -> FastMCP:
    """Create and configure the MCP server.

    Creates a FastMCP server with all Cangjie documentation tools registered.
    The VectorStore is initialized from settings.

    Args:
        settings: Application settings including paths and embedding config

    Returns:
        Configured FastMCP instance ready to serve requests
    """
    mcp = FastMCP(
        name="cangjie_mcp",
        instructions=SERVER_INSTRUCTIONS,
    )

    ctx = tools.create_tool_context(settings)
    _register_tools(mcp, ctx)

    return mcp


def create_mcp_server_with_store(
    settings: Settings,
    store: VectorStore,
    key: IndexKey | None = None,
) -> FastMCP:
    """Create and configure the MCP server with a pre-loaded VectorStore.

    This is used by the HTTP server to create MCP instances for each index,
    where the VectorStore is already loaded by MultiIndexStore.

    Args:
        settings: Application settings
        store: Pre-loaded VectorStore instance
        key: Optional IndexKey for naming the server instance

    Returns:
        Configured FastMCP instance
    """
    server_name = f"cangjie_mcp_{key.version}_{key.lang}" if key else "cangjie_mcp"

    instructions = SERVER_INSTRUCTIONS
    if key:
        instructions = f"Index: {key.version} ({key.lang})\n\n{instructions}"

    mcp = FastMCP(
        name=server_name,
        instructions=instructions,
    )

    ctx = tools.create_tool_context_with_store(settings, store)
    _register_tools(mcp, ctx)

    return mcp
