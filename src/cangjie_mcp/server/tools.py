"""MCP tool definitions for Cangjie documentation server."""

import asyncio
import os
from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from dataclasses import dataclass, field
from pathlib import Path
from typing import Annotated, Any

from mcp.server.fastmcp import Context, FastMCP
from mcp.types import ToolAnnotations
from pydantic import BaseModel, Field

from cangjie_mcp.config import Settings, get_settings
from cangjie_mcp.indexer.document_source import (
    DocumentSource,
    GitDocumentSource,
    NullDocumentSource,
    PrebuiltDocumentSource,
)
from cangjie_mcp.indexer.initializer import initialize_and_index
from cangjie_mcp.indexer.loader import extract_code_blocks
from cangjie_mcp.indexer.store import SearchResult as StoreSearchResult
from cangjie_mcp.indexer.store import VectorStore, create_vector_store
from cangjie_mcp.prebuilt.manager import PrebuiltManager
from cangjie_mcp.prompts import get_prompt
from cangjie_mcp.repo.git_manager import GitManager

# =============================================================================
# Output Models (Pydantic)
# =============================================================================


class SearchResultItem(BaseModel):
    """Single search result item."""

    content: str
    score: float
    file_path: str
    category: str
    topic: str
    title: str


class DocsSearchResult(BaseModel):
    """Search result with pagination metadata."""

    items: list[SearchResultItem]
    total: int
    count: int
    offset: int
    has_more: bool
    next_offset: int | None


class TopicResult(BaseModel):
    """Topic document result type."""

    content: str
    file_path: str
    category: str
    topic: str
    title: str


class CodeExample(BaseModel):
    """Code example type."""

    language: str
    code: str
    context: str
    source_topic: str
    source_file: str


class ToolExample(BaseModel):
    """Tool usage example type."""

    code: str
    context: str


class ToolUsageResult(BaseModel):
    """Tool usage result type."""

    tool_name: str
    content: str
    examples: list[ToolExample]


class TopicsListResult(BaseModel):
    """Topics list result with metadata."""

    categories: dict[str, list[str]]
    total_categories: int
    total_topics: int


class StdlibResultItem(BaseModel):
    """Single stdlib search result item."""

    content: str
    score: float
    file_path: str
    title: str
    packages: list[str]
    type_names: list[str]
    code_examples: list[CodeExample]


class StdlibSearchResult(BaseModel):
    """Stdlib search result."""

    items: list[StdlibResultItem]
    count: int
    detected_packages: list[str]


# =============================================================================
# Tool Context
# =============================================================================


@dataclass
class ToolContext:
    """Context for MCP tools."""

    settings: Settings
    store: VectorStore
    document_source: DocumentSource


@dataclass
class LifespanContext:
    """Application context yielded by the lifespan.

    Yielded immediately so the server can accept connections while
    background initialization is still running.  Tools call
    ``await ready()`` to block until the ``ToolContext`` is available.
    """

    _event: asyncio.Event = field(default_factory=asyncio.Event)
    _tool_ctx: ToolContext | None = field(default=None, init=False)

    async def ready(self) -> ToolContext:
        """Block until initialization is complete, then return the ToolContext."""
        await self._event.wait()
        assert self._tool_ctx is not None
        return self._tool_ctx

    def complete(self, tool_ctx: ToolContext) -> None:
        """Signal that initialization is complete."""
        self._tool_ctx = tool_ctx
        self._event.set()


def create_tool_context(
    settings: Settings,
    store: VectorStore | None = None,
    document_source: DocumentSource | None = None,
) -> ToolContext:
    """Create tool context from settings.

    Args:
        settings: Application settings
        store: Optional pre-loaded VectorStore. If None, creates a new one.
        document_source: Optional DocumentSource. If None, auto-detects the best source.
                        Priority: prebuilt docs > git repo > null source

    Returns:
        ToolContext with initialized components
    """
    if document_source is None:
        document_source = _create_document_source(settings)

    return ToolContext(
        settings=settings,
        store=store if store is not None else create_vector_store(settings),
        document_source=document_source,
    )


def _create_document_source(settings: Settings) -> DocumentSource:
    """Create the best available document source.

    Auto-detects the best source in order:
    1. Prebuilt docs (from installed prebuilt archive)
    2. Git repository (read directly from git without checkout)
    3. Null source (fallback when no docs available)

    Args:
        settings: Application settings

    Returns:
        The best available DocumentSource
    """
    # Try prebuilt docs first
    prebuilt_mgr = PrebuiltManager(settings.data_dir)
    installed = prebuilt_mgr.get_installed_metadata()

    if installed and installed.docs_path:
        docs_dir = Path(installed.docs_path)
        if docs_dir.exists():
            return PrebuiltDocumentSource(docs_dir)

    # Try git source - read directly from git
    git_mgr = GitManager(settings.docs_repo_dir)
    if git_mgr.is_cloned() and git_mgr.repo is not None:
        return GitDocumentSource(
            repo=git_mgr.repo,
            version=settings.docs_version,
            lang=settings.docs_lang,
        )

    # Fallback to null source
    return NullDocumentSource()


# =============================================================================
# MCP Server Instance
# =============================================================================


@asynccontextmanager
async def _lifespan(_server: FastMCP) -> AsyncIterator[LifespanContext]:
    settings = get_settings()
    lifespan_ctx = LifespanContext()

    async def _init() -> None:
        await asyncio.to_thread(initialize_and_index, settings)
        tool_ctx = await asyncio.to_thread(create_tool_context, settings)
        lifespan_ctx.complete(tool_ctx)

    task = asyncio.create_task(_init())
    try:
        yield lifespan_ctx
    finally:
        await task


ANNOTATIONS = ToolAnnotations(
    readOnlyHint=True,
    destructiveHint=False,
    idempotentHint=True,
    openWorldHint=False,
)

mcp = FastMCP(
    "cangjie_mcp",
    lifespan=_lifespan,
    instructions=get_prompt(lsp_enabled=bool(os.environ.get("CANGJIE_HOME"))),
)


# =============================================================================
# Tool Implementations
# =============================================================================


@mcp.tool(name="cangjie_search_docs", annotations=ANNOTATIONS)
async def search_docs(
    query: Annotated[
        str,
        Field(
            description="Search query describing what you're looking for "
            "(e.g., 'how to define a class', 'pattern matching syntax')",
            min_length=1,
            max_length=500,
        ),
    ],
    category: Annotated[
        str,
        Field(
            description="Optional category to filter results (e.g., 'cjpm', 'syntax', 'stdlib')",
        ),
    ] = "",
    top_k: Annotated[
        int,
        Field(
            description="Number of results to return",
            ge=1,
            le=20,
        ),
    ] = 5,
    offset: Annotated[
        int,
        Field(
            description="Number of results to skip for pagination",
            ge=0,
        ),
    ] = 0,
    *,
    ctx: Context[Any, LifespanContext, Any],
) -> DocsSearchResult:
    """Search Cangjie documentation using semantic search.

    Performs vector similarity search across all indexed documentation.
    Returns matching sections ranked by relevance with pagination support.

    Args:
        query: Search query describing what you're looking for
        category: Optional category filter (e.g., 'cjpm', 'syntax')
        top_k: Number of results to return (default: 5, max: 20)
        offset: Pagination offset (default: 0)

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
    tool_ctx = await ctx.request_context.lifespan_context.ready()
    # Request extra results for pagination estimation
    fetch_count = offset + top_k + 1
    results = tool_ctx.store.search(
        query=query,
        category=category or None,
        top_k=fetch_count,
    )

    # Apply offset
    paginated_results = results[offset : offset + top_k]
    has_more = len(results) > offset + top_k

    items = [
        SearchResultItem(
            content=result.text,
            score=result.score,
            file_path=result.metadata.file_path,
            category=result.metadata.category,
            topic=result.metadata.topic,
            title=result.metadata.title,
        )
        for result in paginated_results
    ]

    return DocsSearchResult(
        items=items,
        total=len(results),  # Estimated total
        count=len(items),
        offset=offset,
        has_more=has_more,
        next_offset=offset + len(items) if has_more else None,
    )


@mcp.tool(name="cangjie_get_topic", annotations=ANNOTATIONS)
async def get_topic(
    topic: Annotated[
        str,
        Field(
            description="Topic name - the documentation file name without .md extension "
            "(e.g., 'classes', 'pattern-matching', 'async-programming')",
            min_length=1,
            max_length=200,
        ),
    ],
    category: Annotated[
        str,
        Field(
            description="Optional category to narrow the search (e.g., 'syntax', 'stdlib')",
        ),
    ] = "",
    *,
    ctx: Context[Any, LifespanContext, Any],
) -> TopicResult | str:
    """Get complete documentation for a specific topic.

    Retrieves the full content of a documentation file by topic name.
    Use cangjie_list_topics first to discover available topic names.

    Args:
        topic: Topic name (file name without .md extension)
        category: Optional category to narrow search

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
    tool_ctx = await ctx.request_context.lifespan_context.ready()
    doc = tool_ctx.document_source.get_document_by_topic(topic, category or None)

    if doc is None:
        return f"Topic '{topic}' not found"

    return TopicResult(
        content=doc.text,
        file_path=str(doc.metadata.get("file_path", "")),
        category=str(doc.metadata.get("category", "")),
        topic=str(doc.metadata.get("topic", "")),
        title=str(doc.metadata.get("title", "")),
    )


@mcp.tool(name="cangjie_list_topics", annotations=ANNOTATIONS)
async def list_topics(
    category: Annotated[
        str,
        Field(
            description="Optional category to filter by (e.g., 'cjpm', 'syntax')",
        ),
    ] = "",
    *,
    ctx: Context[Any, LifespanContext, Any],
) -> TopicsListResult:
    """List available documentation topics organized by category.

    Returns all documentation topics, optionally filtered by category.
    Use this to discover topic names for use with cangjie_get_topic.

    Args:
        category: Optional category filter

    Returns:
        TopicsListResult containing:
            - categories: Dict mapping category names to lists of topic names
            - total_categories: Number of categories
            - total_topics: Total number of topics across all categories

    Examples:
        - No params -> Returns all categories and their topics
        - category="cjpm" -> Returns only cjpm-related topics
    """
    tool_ctx = await ctx.request_context.lifespan_context.ready()
    cats = [category] if category else tool_ctx.document_source.get_categories()
    categories = {cat: topics for cat in cats if (topics := tool_ctx.document_source.get_topics_in_category(cat))}

    return TopicsListResult(
        categories=categories,
        total_categories=len(categories),
        total_topics=sum(len(t) for t in categories.values()),
    )


@mcp.tool(name="cangjie_get_code_examples", annotations=ANNOTATIONS)
async def get_code_examples(
    feature: Annotated[
        str,
        Field(
            description="Feature to find examples for (e.g., 'pattern matching', 'async/await', 'generics')",
            min_length=1,
            max_length=200,
        ),
    ],
    top_k: Annotated[
        int,
        Field(
            description="Number of documents to search for examples",
            ge=1,
            le=10,
        ),
    ] = 3,
    *,
    ctx: Context[Any, LifespanContext, Any],
) -> list[CodeExample]:
    """Get code examples for a specific Cangjie language feature.

    Searches documentation for code blocks related to a feature.
    Returns extracted code examples with their surrounding context.

    Args:
        feature: Feature to find examples for
        top_k: Number of documents to search (default: 3)

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
    tool_ctx = await ctx.request_context.lifespan_context.ready()
    results = tool_ctx.store.search(query=feature, top_k=top_k)

    examples: list[CodeExample] = []
    for result in results:
        code_blocks = extract_code_blocks(result.text)

        for block in code_blocks:
            examples.append(
                CodeExample(
                    language=block.language,
                    code=block.code,
                    context=block.context,
                    source_topic=result.metadata.topic,
                    source_file=result.metadata.file_path,
                )
            )

    return examples


@mcp.tool(name="cangjie_get_tool_usage", annotations=ANNOTATIONS)
async def get_tool_usage(
    tool_name: Annotated[
        str,
        Field(
            description="Name of the Cangjie tool (e.g., 'cjc', 'cjpm', 'cjfmt', 'cjcov')",
            min_length=1,
            max_length=50,
        ),
    ],
    *,
    ctx: Context[Any, LifespanContext, Any],
) -> ToolUsageResult | str:
    """Get usage information for Cangjie development tools.

    Searches for documentation about Cangjie CLI tools including
    compiler, package manager, formatter, and other utilities.

    Args:
        tool_name: Name of the tool (e.g., 'cjc', 'cjpm', 'cjfmt')

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
    tool_ctx = await ctx.request_context.lifespan_context.ready()
    results = tool_ctx.store.search(
        query=f"{tool_name} tool usage command",
        top_k=3,
    )

    if not results:
        return f"No usage information found for tool '{tool_name}'"

    combined_content: list[str] = []
    code_examples: list[ToolExample] = []

    for result in results:
        combined_content.append(result.text)

        blocks = extract_code_blocks(result.text)
        for block in blocks:
            if block.language in ("bash", "shell", "sh", ""):
                code_examples.append(
                    ToolExample(
                        code=block.code,
                        context=block.context,
                    )
                )

    return ToolUsageResult(
        tool_name=tool_name,
        content="\n\n---\n\n".join(combined_content),
        examples=code_examples,
    )


@mcp.tool(name="cangjie_search_stdlib", annotations=ANNOTATIONS)
async def search_stdlib(
    query: Annotated[
        str,
        Field(
            description="API name, method, or description to search for "
            "(e.g., 'ArrayList add', 'file read', 'HashMap get')",
            min_length=1,
            max_length=500,
        ),
    ],
    package: Annotated[
        str,
        Field(
            description="Filter by package name (e.g., 'std.collection', 'std.fs', 'std.net'). "
            "Packages are automatically detected from import statements.",
        ),
    ] = "",
    type_name: Annotated[
        str,
        Field(
            description="Filter by type name (e.g., 'ArrayList', 'HashMap', 'File')",
        ),
    ] = "",
    include_examples: Annotated[
        bool,
        Field(
            description="Whether to include code examples in results",
        ),
    ] = True,
    top_k: Annotated[
        int,
        Field(
            description="Number of results to return",
            ge=1,
            le=20,
        ),
    ] = 5,
    *,
    ctx: Context[Any, LifespanContext, Any],
) -> StdlibSearchResult:
    """Search Cangjie standard library APIs.

    Specialized search for standard library documentation.
    Dynamically detects stdlib-related content based on import statements.

    Args:
        query: API name, method, or description to search for
        package: Filter by package (e.g., 'std.collection', 'std.fs')
        type_name: Filter by type (e.g., 'ArrayList', 'HashMap')
        include_examples: Whether to include code examples (default: True)
        top_k: Number of results to return (default: 5)

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
    tool_ctx = await ctx.request_context.lifespan_context.ready()
    # Search with more candidates to allow for filtering
    results = tool_ctx.store.search(query=query, top_k=top_k * 3)

    # Filter to stdlib docs only (using is_stdlib metadata)
    stdlib_results = [
        r
        for r in results
        if r.metadata.file_path  # Has valid metadata
    ]

    # Further filter by package if specified
    if package:
        stdlib_results = [r for r in stdlib_results if _has_package(r, package)]

    # Further filter by type_name if specified
    if type_name:
        stdlib_results = [r for r in stdlib_results if _has_type_name(r, type_name)]

    # Collect all detected packages from results for reference
    all_packages: set[str] = set()

    # Format results
    items: list[StdlibResultItem] = []
    for result in stdlib_results[:top_k]:
        # Get packages from metadata (stored as list)
        packages = _get_list_metadata(result, "packages")
        type_names = _get_list_metadata(result, "type_names")

        all_packages.update(packages)

        # Extract code examples if requested
        code_examples: list[CodeExample] = []
        if include_examples:
            code_blocks = extract_code_blocks(result.text)
            for block in code_blocks:
                code_examples.append(
                    CodeExample(
                        language=block.language,
                        code=block.code,
                        context=block.context,
                        source_topic=result.metadata.topic,
                        source_file=result.metadata.file_path,
                    )
                )

        items.append(
            StdlibResultItem(
                content=result.text,
                score=result.score,
                file_path=result.metadata.file_path,
                title=result.metadata.title,
                packages=packages,
                type_names=type_names,
                code_examples=code_examples,
            )
        )

    return StdlibSearchResult(
        items=items,
        count=len(items),
        detected_packages=sorted(all_packages),
    )


# =============================================================================
# Helper Functions
# =============================================================================


def _get_list_metadata(result: StoreSearchResult, key: str) -> list[str]:
    """Get list metadata from search result by extracting from content.

    Since ChromaDB doesn't store list metadata well, we dynamically extract
    the info from the result text content.

    Args:
        result: Search result from store
        key: Metadata key ("packages" or "type_names")

    Returns:
        List of strings
    """
    from cangjie_mcp.indexer.api_extractor import extract_stdlib_info

    # Extract stdlib info from the result text
    stdlib_info = extract_stdlib_info(result.text)

    if key == "packages":
        return stdlib_info.packages
    elif key == "type_names":
        return stdlib_info.type_names

    return []


def _has_package(result: StoreSearchResult, package: str) -> bool:
    """Check if result contains the specified package.

    Since ChromaDB doesn't store list metadata well, we check the text content.

    Args:
        result: Search result from store
        package: Package name to check for

    Returns:
        True if package is found in the result
    """
    return package in result.text or f"import {package}" in result.text


def _has_type_name(result: StoreSearchResult, type_name: str) -> bool:
    """Check if result contains the specified type name.

    Args:
        result: Search result from store
        type_name: Type name to check for

    Returns:
        True if type name is found in the result
    """
    return type_name in result.text
