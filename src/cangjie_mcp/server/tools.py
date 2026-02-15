"""MCP tool definitions for Cangjie documentation server."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from pydantic import BaseModel, ConfigDict, Field

from cangjie_mcp.config import Settings
from cangjie_mcp.indexer.document_source import (
    DocumentSource,
    GitDocumentSource,
    NullDocumentSource,
    PrebuiltDocumentSource,
)
from cangjie_mcp.indexer.loader import extract_code_blocks
from cangjie_mcp.indexer.store import SearchResult as StoreSearchResult
from cangjie_mcp.indexer.store import VectorStore, create_vector_store
from cangjie_mcp.prebuilt.manager import PrebuiltManager
from cangjie_mcp.repo.git_manager import GitManager

# =============================================================================
# Input Models (Pydantic)
# =============================================================================


class SearchDocsInput(BaseModel):
    """Input model for cangjie_search_docs tool."""

    model_config = ConfigDict(
        str_strip_whitespace=True,
        validate_assignment=True,
        extra="forbid",
    )

    query: str = Field(
        ...,
        description="Search query describing what you're looking for "
        "(e.g., 'how to define a class', 'pattern matching syntax')",
        min_length=1,
        max_length=500,
    )
    category: str | None = Field(
        default=None,
        description="Optional category to filter results (e.g., 'cjpm', 'syntax', 'stdlib')",
    )
    top_k: int = Field(
        default=5,
        description="Number of results to return",
        ge=1,
        le=20,
    )
    offset: int = Field(
        default=0,
        description="Number of results to skip for pagination",
        ge=0,
    )


class GetTopicInput(BaseModel):
    """Input model for cangjie_get_topic tool."""

    model_config = ConfigDict(
        str_strip_whitespace=True,
        validate_assignment=True,
        extra="forbid",
    )

    topic: str = Field(
        ...,
        description="Topic name - the documentation file name without .md extension "
        "(e.g., 'classes', 'pattern-matching', 'async-programming')",
        min_length=1,
        max_length=200,
    )
    category: str | None = Field(
        default=None,
        description="Optional category to narrow the search (e.g., 'syntax', 'stdlib')",
    )


class ListTopicsInput(BaseModel):
    """Input model for cangjie_list_topics tool."""

    model_config = ConfigDict(
        str_strip_whitespace=True,
        validate_assignment=True,
        extra="forbid",
    )

    category: str | None = Field(
        default=None,
        description="Optional category to filter by (e.g., 'cjpm', 'syntax')",
    )


class GetCodeExamplesInput(BaseModel):
    """Input model for cangjie_get_code_examples tool."""

    model_config = ConfigDict(
        str_strip_whitespace=True,
        validate_assignment=True,
        extra="forbid",
    )

    feature: str = Field(
        ...,
        description="Feature to find examples for (e.g., 'pattern matching', 'async/await', 'generics')",
        min_length=1,
        max_length=200,
    )
    top_k: int = Field(
        default=3,
        description="Number of documents to search for examples",
        ge=1,
        le=10,
    )


class GetToolUsageInput(BaseModel):
    """Input model for cangjie_get_tool_usage tool."""

    model_config = ConfigDict(
        str_strip_whitespace=True,
        validate_assignment=True,
        extra="forbid",
    )

    tool_name: str = Field(
        ...,
        description="Name of the Cangjie tool (e.g., 'cjc', 'cjpm', 'cjfmt', 'cjcov')",
        min_length=1,
        max_length=50,
    )


class SearchStdlibInput(BaseModel):
    """Input model for cangjie_search_stdlib tool."""

    model_config = ConfigDict(
        str_strip_whitespace=True,
        validate_assignment=True,
        extra="forbid",
    )

    query: str = Field(
        ...,
        description="API name, method, or description to search for "
        "(e.g., 'ArrayList add', 'file read', 'HashMap get')",
        min_length=1,
        max_length=500,
    )
    package: str | None = Field(
        default=None,
        description="Filter by package name (e.g., 'std.collection', 'std.fs', 'std.net'). "
        "Packages are automatically detected from import statements.",
    )
    type_name: str | None = Field(
        default=None,
        description="Filter by type name (e.g., 'ArrayList', 'HashMap', 'File')",
    )
    include_examples: bool = Field(
        default=True,
        description="Whether to include code examples in results",
    )
    top_k: int = Field(
        default=5,
        description="Number of results to return",
        ge=1,
        le=20,
    )


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
# Tool Implementations
# =============================================================================


def search_docs(ctx: ToolContext, params: SearchDocsInput) -> DocsSearchResult:
    """Search documentation using semantic search.

    Performs semantic search across Cangjie documentation using vector embeddings.
    Returns matching documentation sections with relevance scores and pagination.

    Args:
        ctx: Tool context with store and settings
        params: Validated search parameters

    Returns:
        SearchResult with items and pagination metadata:
        {
            "items": [...],      # List of matching documents
            "total": int,        # Total matches found (estimated)
            "count": int,        # Number of items in this response
            "offset": int,       # Current pagination offset
            "has_more": bool,    # Whether more results are available
            "next_offset": int   # Next offset for pagination (or None)
        }
    """
    # Request extra results for pagination estimation
    fetch_count = params.offset + params.top_k + 1
    results = ctx.store.search(
        query=params.query,
        category=params.category,
        top_k=fetch_count,
    )

    # Apply offset
    paginated_results = results[params.offset : params.offset + params.top_k]
    has_more = len(results) > params.offset + params.top_k

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
        offset=params.offset,
        has_more=has_more,
        next_offset=params.offset + len(items) if has_more else None,
    )


def get_topic(ctx: ToolContext, params: GetTopicInput) -> TopicResult | None:
    """Get complete document for a specific topic.

    Retrieves the full documentation content for a named topic.
    Use list_topics first to discover available topic names.

    Args:
        ctx: Tool context
        params: Validated input with topic name and optional category

    Returns:
        TopicResult with full document content, or None if not found
    """
    doc = ctx.document_source.get_document_by_topic(params.topic, params.category)

    if doc is None:
        return None

    return TopicResult(
        content=doc.text,
        file_path=str(doc.metadata.get("file_path", "")),
        category=str(doc.metadata.get("category", "")),
        topic=str(doc.metadata.get("topic", "")),
        title=str(doc.metadata.get("title", "")),
    )


def list_topics(ctx: ToolContext, params: ListTopicsInput) -> TopicsListResult:
    """List available topics, optionally filtered by category.

    Returns all available documentation topics organized by category.
    Use this to discover topic names for use with get_topic.

    Args:
        ctx: Tool context
        params: Validated input with optional category filter

    Returns:
        TopicsListResult with categories mapping and counts
    """
    cats = [params.category] if params.category else ctx.document_source.get_categories()
    categories = {cat: topics for cat in cats if (topics := ctx.document_source.get_topics_in_category(cat))}

    return TopicsListResult(
        categories=categories,
        total_categories=len(categories),
        total_topics=sum(len(t) for t in categories.values()),
    )


def get_code_examples(ctx: ToolContext, params: GetCodeExamplesInput) -> list[CodeExample]:
    """Get code examples for a specific feature.

    Searches documentation for code examples related to a feature.
    Returns code blocks with their surrounding context.

    Args:
        ctx: Tool context
        params: Validated input with feature name

    Returns:
        List of CodeExample objects with language, code, and source info
    """
    results = ctx.store.search(query=params.feature, top_k=params.top_k)

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


def get_tool_usage(ctx: ToolContext, params: GetToolUsageInput) -> ToolUsageResult | None:
    """Get usage information for a specific Cangjie tool/command.

    Searches for documentation about Cangjie development tools like
    cjc (compiler), cjpm (package manager), cjfmt (formatter), etc.

    Args:
        ctx: Tool context
        params: Validated input with tool name

    Returns:
        ToolUsageResult with documentation and shell examples, or None if not found
    """
    results = ctx.store.search(
        query=f"{params.tool_name} tool usage command",
        top_k=3,
    )

    if not results:
        return None

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
        tool_name=params.tool_name,
        content="\n\n---\n\n".join(combined_content),
        examples=code_examples,
    )


def search_stdlib(ctx: ToolContext, params: SearchStdlibInput) -> StdlibSearchResult:
    """Search Cangjie standard library APIs.

    Performs semantic search filtered to stdlib documentation.
    Dynamically filters results based on is_stdlib metadata that was
    extracted from import statements at index time.

    Args:
        ctx: Tool context with store and settings
        params: Validated search parameters including:
            - query: API name, method, or description
            - package: Optional package filter (e.g., 'std.collection')
            - type_name: Optional type filter (e.g., 'ArrayList')
            - include_examples: Whether to include code examples
            - top_k: Number of results to return

    Returns:
        StdlibSearchResult with filtered stdlib API documentation
    """
    # Search with more candidates to allow for filtering
    results = ctx.store.search(query=params.query, top_k=params.top_k * 3)

    # Filter to stdlib docs only (using is_stdlib metadata)
    stdlib_results = [
        r
        for r in results
        if r.metadata.file_path  # Has valid metadata
    ]

    # Further filter by package if specified
    if params.package:
        stdlib_results = [r for r in stdlib_results if _has_package(r, params.package)]

    # Further filter by type_name if specified
    if params.type_name:
        stdlib_results = [r for r in stdlib_results if _has_type_name(r, params.type_name)]

    # Collect all detected packages from results for reference
    all_packages: set[str] = set()

    # Format results
    items: list[StdlibResultItem] = []
    for result in stdlib_results[: params.top_k]:
        # Get packages from metadata (stored as list)
        packages = _get_list_metadata(result, "packages")
        type_names = _get_list_metadata(result, "type_names")

        all_packages.update(packages)

        # Extract code examples if requested
        code_examples: list[CodeExample] = []
        if params.include_examples:
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
