"""MCP tool definitions for Cangjie documentation server."""

from dataclasses import dataclass
from typing import TypedDict

from cangjie_mcp.config import Settings
from cangjie_mcp.indexer.embeddings import get_embedding_provider
from cangjie_mcp.indexer.loader import DocumentLoader, extract_code_blocks
from cangjie_mcp.indexer.store import VectorStore


class SearchResult(TypedDict):
    """Search result type."""

    content: str
    score: float
    file_path: str
    category: str
    topic: str
    title: str


class TopicResult(TypedDict):
    """Topic document result type."""

    content: str
    file_path: str
    category: str
    topic: str
    title: str


class CodeExample(TypedDict):
    """Code example type."""

    language: str
    code: str
    context: str
    source_topic: str
    source_file: str


class ToolExample(TypedDict):
    """Tool usage example type."""

    code: str
    context: str


class ToolUsageResult(TypedDict):
    """Tool usage result type."""

    tool_name: str
    content: str
    examples: list[ToolExample]


@dataclass
class ToolContext:
    """Context for MCP tools."""

    settings: Settings
    store: VectorStore
    loader: DocumentLoader


def create_tool_context(settings: Settings) -> ToolContext:
    """Create tool context from settings.

    Args:
        settings: Application settings

    Returns:
        ToolContext with initialized components
    """
    embedding_provider = get_embedding_provider(settings)
    store = VectorStore(
        db_path=settings.chroma_db_dir,
        embedding_provider=embedding_provider,
    )
    loader = DocumentLoader(settings.docs_source_dir)

    return ToolContext(
        settings=settings,
        store=store,
        loader=loader,
    )


def search_docs(
    ctx: ToolContext,
    query: str,
    category: str | None = None,
    top_k: int = 5,
) -> list[SearchResult]:
    """Search documentation using semantic search.

    Args:
        ctx: Tool context
        query: Search query
        category: Optional category to filter by
        top_k: Number of results to return

    Returns:
        List of search results
    """
    results = ctx.store.search(query=query, category=category, top_k=top_k)

    return [
        SearchResult(
            content=result.text,
            score=result.score,
            file_path=result.metadata.file_path,
            category=result.metadata.category,
            topic=result.metadata.topic,
            title=result.metadata.title,
        )
        for result in results
    ]


def get_topic(ctx: ToolContext, topic: str, category: str | None = None) -> TopicResult | None:
    """Get complete document for a specific topic.

    Args:
        ctx: Tool context
        topic: Topic name (file stem)
        category: Optional category to narrow search

    Returns:
        Document content and metadata, or None if not found
    """
    doc = ctx.loader.get_document_by_topic(topic, category)

    if doc is None:
        return None

    return TopicResult(
        content=doc.text,
        file_path=str(doc.metadata.get("file_path", "")),
        category=str(doc.metadata.get("category", "")),
        topic=str(doc.metadata.get("topic", "")),
        title=str(doc.metadata.get("title", "")),
    )


def list_topics(ctx: ToolContext, category: str | None = None) -> dict[str, list[str]]:
    """List available topics, optionally filtered by category.

    Args:
        ctx: Tool context
        category: Optional category to filter by

    Returns:
        Dictionary mapping categories to topic lists
    """
    if category:
        topics = ctx.loader.get_topics_in_category(category)
        return {category: topics}

    # Get all categories and their topics
    result = {}
    for cat in ctx.loader.get_categories():
        topics = ctx.loader.get_topics_in_category(cat)
        if topics:
            result[cat] = topics

    return result


def get_code_examples(
    ctx: ToolContext,
    feature: str,
    top_k: int = 3,
) -> list[CodeExample]:
    """Get code examples for a specific feature.

    Args:
        ctx: Tool context
        feature: Feature to search for
        top_k: Number of documents to search

    Returns:
        List of code examples with context
    """
    # Search for documents related to the feature
    results = ctx.store.search(query=feature, top_k=top_k)

    examples: list[CodeExample] = []
    for result in results:
        # Extract code blocks from the result text
        code_blocks = extract_code_blocks(result.text)

        for block in code_blocks:
            examples.append(CodeExample(
                language=block.language,
                code=block.code,
                context=block.context,
                source_topic=result.metadata.topic,
                source_file=result.metadata.file_path,
            ))

    return examples


def get_tool_usage(ctx: ToolContext, tool_name: str) -> ToolUsageResult | None:
    """Get usage information for a specific tool/command.

    Args:
        ctx: Tool context
        tool_name: Name of the tool (e.g., cjc, cjpm)

    Returns:
        Tool usage information or None if not found
    """
    # Search for tool documentation
    results = ctx.store.search(
        query=f"{tool_name} tool usage command",
        top_k=3,
    )

    if not results:
        return None

    # Combine relevant results
    combined_content: list[str] = []
    code_examples: list[ToolExample] = []

    for result in results:
        combined_content.append(result.text)

        # Extract code examples
        blocks = extract_code_blocks(result.text)
        for block in blocks:
            if block.language in ("bash", "shell", "sh", ""):
                code_examples.append(ToolExample(
                    code=block.code,
                    context=block.context,
                ))

    return ToolUsageResult(
        tool_name=tool_name,
        content="\n\n---\n\n".join(combined_content),
        examples=code_examples,
    )
