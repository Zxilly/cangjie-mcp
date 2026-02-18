"""HTTP query server for cangjie-mcp.

Starlette-based HTTP server that exposes the search index and document
source over HTTP. Used in ``cangjie-mcp server`` mode to serve queries
to remote MCP clients.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from starlette.applications import Starlette
from starlette.requests import Request
from starlette.responses import JSONResponse
from starlette.routing import Route

from cangjie_mcp.utils import logger

if TYPE_CHECKING:
    from cangjie_mcp.indexer.document_source import DocumentSource
    from cangjie_mcp.indexer.search_index import SearchIndex
    from cangjie_mcp.indexer.store import IndexMetadata


def create_http_app(
    search_index: SearchIndex,
    document_source: DocumentSource,
    index_metadata: IndexMetadata,
) -> Starlette:
    """Create a Starlette application that serves the search index over HTTP.

    Args:
        search_index: Initialized SearchIndex for handling queries.
        document_source: DocumentSource for topic browsing.
        index_metadata: Index metadata (version, lang, embedding_model).

    Returns:
        Configured Starlette application.
    """
    # Pre-compute the topics listing at startup. The git tree is immutable
    # during the server's lifetime, so this avoids expensive per-request
    # git traversal that can take tens of seconds on large repos.
    _topics_cache: dict[str, list[dict[str, str]]] = {}
    for cat in document_source.get_categories():
        titles = document_source.get_topic_titles(cat)
        _topics_cache[cat] = [
            {"name": t, "title": titles.get(t, "")} for t in document_source.get_topics_in_category(cat)
        ]
    _total_topics = sum(len(t) for t in _topics_cache.values())
    logger.info(
        "Topics cache built: %d categories, %d topics",
        len(_topics_cache),
        _total_topics,
    )

    async def health(_request: Request) -> JSONResponse:
        return JSONResponse({"status": "ok"})

    async def info(_request: Request) -> JSONResponse:
        return JSONResponse(
            {
                "version": index_metadata.version,
                "lang": index_metadata.lang,
                "embedding_model": index_metadata.embedding_model,
                "document_count": index_metadata.document_count,
            }
        )

    async def search(request: Request) -> JSONResponse:
        body = await request.json()
        query: str = body.get("query", "")
        if not query:
            return JSONResponse({"error": "query is required"}, status_code=400)

        top_k: int = body.get("top_k", 5)
        category: str | None = body.get("category")
        rerank: bool = body.get("rerank", True)

        results = await search_index.query(
            query=query,
            top_k=top_k,
            category=category,
            rerank=rerank,
        )

        return JSONResponse(
            {
                "results": [
                    {
                        "text": r.text,
                        "score": r.score,
                        "metadata": {
                            "file_path": r.metadata.file_path,
                            "category": r.metadata.category,
                            "topic": r.metadata.topic,
                            "title": r.metadata.title,
                            "has_code": r.metadata.has_code,
                        },
                    }
                    for r in results
                ],
            }
        )

    async def topics(_request: Request) -> JSONResponse:
        return JSONResponse({"categories": _topics_cache})

    def topic_detail(request: Request) -> JSONResponse:
        category = request.path_params["category"]
        topic = request.path_params["topic"]

        doc = document_source.get_document_by_topic(topic, category)
        if doc is None:
            return JSONResponse({"error": "not found"}, status_code=404)

        return JSONResponse(
            {
                "content": doc.text,
                "file_path": str(doc.metadata.get("file_path", "")),
                "category": str(doc.metadata.get("category", "")),
                "topic": str(doc.metadata.get("topic", "")),
                "title": str(doc.metadata.get("title", "")),
            }
        )

    routes = [
        Route("/health", health, methods=["GET"]),
        Route("/info", info, methods=["GET"]),
        Route("/search", search, methods=["POST"]),
        Route("/topics", topics, methods=["GET"]),
        Route("/topics/{category}/{topic}", topic_detail, methods=["GET"]),
    ]

    return Starlette(routes=routes)
