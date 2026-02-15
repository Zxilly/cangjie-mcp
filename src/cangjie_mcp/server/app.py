"""FastMCP tool registration for Cangjie documentation."""

from typing import TYPE_CHECKING

from mcp.server.fastmcp import FastMCP
from mcp.types import ToolAnnotations

from cangjie_mcp.server import tools
from cangjie_mcp.server.tools import (
    CodeExample,
    DocsSearchResult,
    GetCodeExamplesInput,
    GetToolUsageInput,
    GetTopicInput,
    ListTopicsInput,
    SearchDocsInput,
    SearchStdlibInput,
    StdlibSearchResult,
    ToolUsageResult,
    TopicResult,
    TopicsListResult,
)

if TYPE_CHECKING:
    from cangjie_mcp.server.factory import InitGate

# =============================================================================
# Tool Registration
# =============================================================================


def register_docs_tools(mcp: FastMCP, gate: "InitGate") -> None:
    """Register all MCP tools with the server.

    Each tool awaits the gate before processing, so the server can accept
    connections immediately while initialization runs in the background.

    Args:
        mcp: FastMCP server instance
        gate: Initialization gate that resolves to ToolContext when ready
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
    async def cangjie_search_docs(params: SearchDocsInput) -> DocsSearchResult:
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
            DocsSearchResult containing:
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
        ctx = await gate.get()
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
    async def cangjie_get_topic(params: GetTopicInput) -> TopicResult | str:
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
        ctx = await gate.get()
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
    async def cangjie_list_topics(params: ListTopicsInput) -> TopicsListResult:
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
        ctx = await gate.get()
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
    async def cangjie_get_code_examples(params: GetCodeExamplesInput) -> list[CodeExample]:
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
        ctx = await gate.get()
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
    async def cangjie_get_tool_usage(params: GetToolUsageInput) -> ToolUsageResult | str:
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
        ctx = await gate.get()
        result = tools.get_tool_usage(ctx, params)
        return result if result else f"No usage information found for tool '{params.tool_name}'"

    @mcp.tool(
        name="cangjie_search_stdlib",
        annotations=ToolAnnotations(
            title="Search Standard Library APIs",
            readOnlyHint=True,
            destructiveHint=False,
            idempotentHint=True,
            openWorldHint=False,
        ),
    )
    async def cangjie_search_stdlib(params: SearchStdlibInput) -> StdlibSearchResult:
        """Search Cangjie standard library APIs.

        Specialized search for standard library documentation.
        Dynamically detects stdlib-related content based on import statements.

        Args:
            params: Search parameters including:
                - query (str): API name, method, or description to search for
                - package (str | None): Filter by package (e.g., 'std.collection', 'std.fs')
                - type_name (str | None): Filter by type (e.g., 'ArrayList', 'HashMap')
                - include_examples (bool): Whether to include code examples (default: True)
                - top_k (int): Number of results to return (default: 5)

        Returns:
            StdlibSearchResult containing:
                - items: List of stdlib API docs with content, packages, types, examples
                - count: Number of results
                - detected_packages: List of all packages found in results

        Examples:
            - query="ArrayList add" -> ArrayList methods documentation
            - query="file read", package="std.fs" -> File I/O docs
            - query="HashMap get", type_name="HashMap" -> HashMap-specific docs
        """
        ctx = await gate.get()
        return tools.search_stdlib(ctx, params)
