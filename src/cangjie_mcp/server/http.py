"""HTTP server with multi-index support for Cangjie MCP."""

from __future__ import annotations

import re
from collections.abc import Callable, Coroutine
from typing import Any

from mcp.server.fastmcp import FastMCP
from rich.console import Console
from starlette.applications import Starlette
from starlette.requests import Request
from starlette.responses import JSONResponse, Response
from starlette.routing import Mount, Route

from cangjie_mcp.config import IndexKey, Settings
from cangjie_mcp.indexer.multi_store import (
    LoadedIndex,
    MultiIndexStore,
    create_multi_index_store,
)
from cangjie_mcp.server.app import create_mcp_server_with_store

console = Console()

# Type alias for exception handler
ExceptionHandler = Callable[[Request, Exception], Coroutine[Any, Any, Response]]

# Pattern to match index paths: /{version}/{lang}/...
INDEX_PATH_PATTERN = re.compile(r"^/([^/]+)/([^/]+)(?:/.*)?$")


class MultiIndexHTTPServer:
    """HTTP server supporting multiple indexes with path-based routing.

    This server downloads prebuilt indexes from URLs, creates separate MCP
    instances for each, and routes requests based on version/language paths.

    Routes:
        GET  /health             -> Health check
        GET  /indexes            -> List loaded indexes
        POST /{version}/{lang}/mcp  -> Streamable HTTP MCP endpoint (404 if not loaded)

    Any request to /{version}/{lang}/... where the index is not loaded
    will return a 404 response.
    """

    def __init__(
        self,
        settings: Settings,
        index_urls: list[str],
    ) -> None:
        """Initialize multi-index HTTP server.

        Args:
            settings: Application settings
            index_urls: List of URLs to prebuilt index archives
        """
        self.settings = settings
        self.index_urls = index_urls
        self._multi_store: MultiIndexStore | None = None
        self._loaded_indexes: dict[IndexKey, LoadedIndex] = {}
        self._mcp_servers: dict[IndexKey, FastMCP] = {}
        self._app: Starlette | None = None

    def _load_indexes(self) -> None:
        """Load all indexes from configured URLs."""
        self._multi_store = create_multi_index_store(self.settings)
        self._loaded_indexes = self._multi_store.load_from_urls(self.index_urls)

        # Create MCP servers for each loaded index
        for key, loaded in self._loaded_indexes.items():
            mcp = create_mcp_server_with_store(self.settings, loaded.store, key)
            self._mcp_servers[key] = mcp

    async def _health_check(self, _request: Request) -> Response:
        """Health check endpoint."""
        return JSONResponse(
            {
                "status": "healthy",
                "indexes": [str(k) for k in self._mcp_servers],
            }
        )

    async def _list_indexes(self, _request: Request) -> Response:
        """List loaded indexes endpoint."""
        indexes = []
        for key, loaded in self._loaded_indexes.items():
            indexes.append(
                {
                    "version": key.version,
                    "lang": key.lang,
                    "path": f"/{key.path_segment}",
                    "mcp_endpoint": f"/{key.path_segment}/mcp",
                    "source_url": loaded.url,
                    "embedding_model": loaded.metadata.embedding_model,
                }
            )

        return JSONResponse(
            {
                "indexes": indexes,
                "total": len(indexes),
            }
        )

    def _create_404_handler(self) -> ExceptionHandler:
        """Create a 404 exception handler with access to server state."""
        mcp_servers = self._mcp_servers

        async def handle_404(request: Request, _exc: Exception) -> Response:
            """Handle 404 errors with helpful messages for index paths."""
            path = request.url.path

            # Check if this looks like an index path
            match = INDEX_PATH_PATTERN.match(path)
            if match:
                version, lang = match.groups()
                available = [str(k) for k in mcp_servers]

                return JSONResponse(
                    {
                        "error": "Index not found",
                        "requested": f"{version}:{lang}",
                        "message": f"No index loaded for version '{version}' and language '{lang}'",
                        "available_indexes": available,
                    },
                    status_code=404,
                )

            # Generic 404
            return JSONResponse(
                {
                    "error": "Not found",
                    "path": path,
                },
                status_code=404,
            )

        return handle_404

    def _create_app(self) -> Starlette:
        """Create the Starlette ASGI application."""
        routes: list[Route | Mount] = [
            Route("/health", self._health_check, methods=["GET"]),
            Route("/indexes", self._list_indexes, methods=["GET"]),
        ]

        # Mount MCP servers for each index
        for key, mcp in self._mcp_servers.items():
            # Get the ASGI app from FastMCP
            mcp_app = mcp.streamable_http_app()

            # Mount at /{version}/{lang}/
            mount_path = f"/{key.path_segment}"
            routes.append(Mount(mount_path, app=mcp_app))
            console.print(f"  [blue]Mounted: {mount_path}[/blue]")

        # Create app with custom exception handlers
        exception_handlers = {
            404: self._create_404_handler(),
        }

        return Starlette(routes=routes, exception_handlers=exception_handlers)

    def create_app(self) -> Starlette:
        """Create and return the ASGI application.

        This method loads indexes and creates the Starlette app.
        Can be used for testing or custom ASGI server integration.

        Returns:
            Configured Starlette ASGI application
        """
        if self._app is None:
            self._load_indexes()
            self._app = self._create_app()
        return self._app

    def run(self) -> None:
        """Run the HTTP server using uvicorn."""
        import uvicorn

        app = self.create_app()

        console.print()
        console.print("[bold green]Server ready![/bold green]")
        console.print(
            f"  Health: http://{self.settings.http_host}:{self.settings.http_port}/health"
        )
        console.print(
            f"  Indexes: http://{self.settings.http_host}:{self.settings.http_port}/indexes"
        )
        for key in self._mcp_servers:
            console.print(
                f"  MCP ({key}): "
                f"http://{self.settings.http_host}:{self.settings.http_port}/{key.path_segment}/mcp"
            )
        console.print()

        uvicorn.run(
            app,
            host=self.settings.http_host,
            port=self.settings.http_port,
            log_level="info",
        )
