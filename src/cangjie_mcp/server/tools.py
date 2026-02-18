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

from cangjie_mcp.config import IndexInfo, Settings, get_settings
from cangjie_mcp.indexer.document_source import (
    DocumentSource,
    GitDocumentSource,
    RemoteDocumentSource,
)
from cangjie_mcp.indexer.loader import extract_code_blocks
from cangjie_mcp.indexer.search_index import LocalSearchIndex, RemoteSearchIndex, SearchIndex
from cangjie_mcp.indexer.store import SearchResult as StoreSearchResult
from cangjie_mcp.prompts import get_prompt
from cangjie_mcp.utils import logger

# =============================================================================
# Output Models (Pydantic)
# =============================================================================


class CodeExample(BaseModel):
    """Code example type."""

    language: str
    code: str
    context: str
    source_topic: str
    source_file: str


class SearchResultItem(BaseModel):
    """Single search result item."""

    content: str
    score: float
    file_path: str
    category: str
    topic: str
    title: str
    has_code_examples: bool = False
    code_examples: list[CodeExample] | None = None


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


class TopicInfo(BaseModel):
    """Topic name with title summary."""

    name: str
    title: str


class TopicsListResult(BaseModel):
    """Topics list result with metadata."""

    categories: dict[str, list[TopicInfo]]
    total_categories: int
    total_topics: int
    error: str | None = None
    available_categories: list[str] | None = None


# =============================================================================
# Tool Context
# =============================================================================


@dataclass
class ToolContext:
    """Context for MCP tools."""

    settings: Settings
    index_info: IndexInfo
    search_index: SearchIndex
    document_source: DocumentSource


@dataclass
class LifespanContext:
    """Application context yielded by the lifespan.

    Yielded immediately so the server can accept connections while
    background initialization is still running.  Tools call
    ``await ready()`` to block until the ``ToolContext`` is available.
    LSP tools call ``await lsp_ready()`` to block until the LSP client
    is available.
    """

    _event: asyncio.Event = field(default_factory=asyncio.Event)
    _tool_ctx: ToolContext | None = field(default=None, init=False)
    _error: BaseException | None = field(default=None, init=False)

    # LSP lazy loading state
    _lsp_event: asyncio.Event = field(default_factory=asyncio.Event)
    lsp_available: bool = field(default=False, init=False)

    async def ready(self) -> ToolContext:
        """Block until initialization is complete, then return the ToolContext."""
        await self._event.wait()
        if self._error is not None:
            raise RuntimeError(f"Initialization failed: {self._error}") from self._error
        assert self._tool_ctx is not None
        return self._tool_ctx

    def complete(self, tool_ctx: ToolContext) -> None:
        """Signal that initialization is complete."""
        self._tool_ctx = tool_ctx
        self._event.set()

    def fail(self, error: BaseException) -> None:
        """Signal that initialization failed with an error."""
        self._error = error
        self._event.set()

    async def lsp_ready(self) -> bool:
        """Block until LSP initialization is complete, then return availability."""
        await self._lsp_event.wait()
        return self.lsp_available

    def lsp_complete(self, available: bool) -> None:
        """Signal that LSP initialization is complete."""
        self.lsp_available = available
        self._lsp_event.set()


def create_document_source(settings: Settings, index_info: IndexInfo) -> DocumentSource:
    """Create a document source backed by the git repository.

    Args:
        settings: Application settings
        index_info: Index identity and paths

    Returns:
        GitDocumentSource for the indexed repository

    Raises:
        RuntimeError: If the documentation repository is not available
    """
    from cangjie_mcp.repo.git_manager import GitManager

    git_mgr = GitManager(settings.docs_repo_dir)
    if not git_mgr.is_cloned() or git_mgr.repo is None:
        raise RuntimeError(
            f"Documentation repository not found at {settings.docs_repo_dir}. "
            "The index was built but the git repo is missing."
        )

    return GitDocumentSource(
        repo=git_mgr.repo,
        version=index_info.version,
        lang=index_info.lang,
    )


# =============================================================================
# MCP Server Instance
# =============================================================================


@asynccontextmanager
async def _lifespan(_server: FastMCP) -> AsyncIterator[LifespanContext]:
    settings = get_settings()
    lifespan_ctx = LifespanContext()

    async def _init() -> None:
        try:
            if settings.server_url:
                search_index: SearchIndex = RemoteSearchIndex(settings.server_url)
            else:
                search_index = LocalSearchIndex(settings)

            logger.info("Initializing index...")
            index_info = await asyncio.to_thread(search_index.init)
            from cangjie_mcp.config import format_startup_info

            logger.info(format_startup_info(settings, index_info))

            if settings.server_url:
                doc_source: DocumentSource = RemoteDocumentSource(settings.server_url)
            else:
                doc_source = create_document_source(settings, index_info)

            tool_ctx = ToolContext(
                settings=settings,
                index_info=index_info,
                search_index=search_index,
                document_source=doc_source,
            )
            lifespan_ctx.complete(tool_ctx)
            logger.info("Initialization complete — tools are ready.")
        except BaseException as e:
            logger.exception("Initialization failed")
            lifespan_ctx.fail(e)
            if not isinstance(e, Exception):
                raise

    async def _init_lsp() -> None:
        cangjie_home = os.environ.get("CANGJIE_HOME")
        if not cangjie_home:
            logger.warning("CANGJIE_HOME not set — LSP tools will not be available.")
            lifespan_ctx.lsp_complete(False)
            return
        try:
            from cangjie_mcp.lsp import init as lsp_init
            from cangjie_mcp.lsp.config import LSPSettings

            workspace = os.environ.get("CANGJIE_WORKSPACE", "")
            lsp_settings = LSPSettings(
                sdk_path=Path(cangjie_home),
                workspace_path=Path(workspace) if workspace else Path.cwd(),
            )
            success = await lsp_init(lsp_settings)
            lifespan_ctx.lsp_complete(success)
        except Exception as e:
            logger.error(f"Failed to initialize LSP: {e}")
            lifespan_ctx.lsp_complete(False)

    task = asyncio.create_task(_init())
    lsp_task = asyncio.create_task(_init_lsp())
    try:
        yield lifespan_ctx
    finally:
        await task
        await lsp_task
        if lifespan_ctx.lsp_available:
            from cangjie_mcp.lsp import shutdown as lsp_shutdown

            await lsp_shutdown()


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
    extract_code: Annotated[
        bool,
        Field(
            description="Whether to extract code examples from results",
        ),
    ] = False,
    package: Annotated[
        str,
        Field(
            description="Filter by stdlib package name (e.g., 'std.collection', 'std.fs')",
        ),
    ] = "",
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
        extract_code: Whether to extract code examples from results (default: False)
        package: Filter by stdlib package name (e.g., 'std.collection', 'std.fs')

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
        - Query: "generics", extract_code=True -> Returns docs with code examples
        - Query: "ArrayList", package="std.collection" -> Filters to stdlib package
    """
    tool_ctx = await ctx.request_context.lifespan_context.ready()

    # When filtering by package, fetch extra candidates to allow for post-filtering
    fetch_multiplier = 3 if package else 1
    fetch_count = (offset + top_k + 1) * fetch_multiplier
    results = await tool_ctx.search_index.query(
        query=query,
        category=category or None,
        top_k=fetch_count,
    )

    # Filter by package if specified
    if package:
        results = [r for r in results if _has_package(r, package)]

    # Apply offset
    paginated_results = results[offset : offset + top_k]
    has_more = len(results) > offset + top_k

    items: list[SearchResultItem] = []
    for result in paginated_results:
        code_examples: list[CodeExample] | None = None
        if extract_code:
            code_blocks = extract_code_blocks(result.text)
            code_examples = [
                CodeExample(
                    language=block.language,
                    code=block.code,
                    context=block.context,
                    source_topic=result.metadata.topic,
                    source_file=result.metadata.file_path,
                )
                for block in code_blocks
            ]
        items.append(
            SearchResultItem(
                content=result.text,
                score=result.score,
                file_path=result.metadata.file_path,
                category=result.metadata.category,
                topic=result.metadata.topic,
                title=result.metadata.title,
                has_code_examples=result.metadata.has_code,
                code_examples=code_examples,
            )
        )

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
    try:
        doc = await asyncio.wait_for(
            asyncio.to_thread(tool_ctx.document_source.get_document_by_topic, topic, category or None),
            timeout=60.0,
        )
    except TimeoutError:
        logger.warning("get_topic timed out: topic=%s, category=%s", topic, category)
        return f"Topic lookup timed out for '{topic}'. The server may be overloaded."
    except Exception as e:
        logger.exception("get_topic failed: topic=%s, category=%s", topic, category)
        return f"Error retrieving topic '{topic}': {e}"

    if doc is None:
        import difflib

        doc_source = tool_ctx.document_source
        all_topics = await asyncio.to_thread(doc_source.get_all_topic_names)
        suggestions = difflib.get_close_matches(topic, all_topics, n=5, cutoff=0.4)

        parts = [f"Topic '{topic}' not found."]
        if suggestions:
            parts.append(f"Did you mean: {', '.join(suggestions)}?")
        if category:
            cat_topics = await asyncio.to_thread(doc_source.get_topics_in_category, category)
            if cat_topics:
                parts.append(f"Available in '{category}': {', '.join(cat_topics[:20])}")
            else:
                cats = await asyncio.to_thread(doc_source.get_categories)
                parts.append(f"Category '{category}' not found. Available: {', '.join(cats)}")
        return "\n".join(parts)

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
    doc_source = tool_ctx.document_source

    # Check category existence upfront when a filter is specified
    if category:
        all_cats = await asyncio.to_thread(doc_source.get_categories)
        if category not in all_cats:
            return TopicsListResult(
                categories={},
                total_categories=0,
                total_topics=0,
                error=f"Category '{category}' not found.",
                available_categories=all_cats,
            )

    def _build_topics_list() -> dict[str, list[TopicInfo]]:
        cats = [category] if category else doc_source.get_categories()
        result: dict[str, list[TopicInfo]] = {}
        for cat in cats:
            topics = doc_source.get_topics_in_category(cat)
            if topics:
                titles = doc_source.get_topic_titles(cat)
                result[cat] = [TopicInfo(name=t, title=titles.get(t, "")) for t in topics]
        return result

    categories: dict[str, list[TopicInfo]] = {}
    try:
        categories = await asyncio.wait_for(
            asyncio.to_thread(_build_topics_list),
            timeout=60.0,
        )
    except TimeoutError:
        logger.warning("list_topics timed out: category=%s", category)
    except Exception:
        logger.exception("list_topics failed: category=%s", category)

    return TopicsListResult(
        categories=categories,
        total_categories=len(categories),
        total_topics=sum(len(t) for t in categories.values()),
    )


# =============================================================================
# Helper Functions
# =============================================================================


def _has_package(result: StoreSearchResult, package: str) -> bool:
    """Check if result contains the specified package.

    Args:
        result: Search result from store
        package: Package name to check for

    Returns:
        True if package is found in the result
    """
    return package in result.text or f"import {package}" in result.text
