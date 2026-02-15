"""MCP server factory.

This module creates and configures the single MCP server instance.
LSP tools are conditionally registered when CANGJIE_HOME is set.
"""

from __future__ import annotations

import asyncio
import contextlib
import os
from collections.abc import AsyncIterator
from contextlib import asynccontextmanager

from mcp.server.fastmcp import FastMCP

from cangjie_mcp.config import Settings
from cangjie_mcp.indexer.initializer import initialize_and_index
from cangjie_mcp.prompts import get_prompt
from cangjie_mcp.server import tools
from cangjie_mcp.server.app import register_docs_tools
from cangjie_mcp.server.lsp_app import register_lsp_tools
from cangjie_mcp.server.tools import ToolContext
from cangjie_mcp.utils import logger


class InitGate:
    """Gates tool access until background initialization completes."""

    def __init__(self) -> None:
        self._ctx: ToolContext | None = None
        self._error: Exception | None = None
        self._event: asyncio.Event | None = None

    @property
    def event(self) -> asyncio.Event:
        if self._event is None:
            self._event = asyncio.Event()
        return self._event

    def set_ready(self, ctx: ToolContext) -> None:
        self._ctx = ctx
        self.event.set()

    def set_error(self, error: Exception) -> None:
        self._error = error
        self.event.set()

    async def get(self) -> ToolContext:
        await self.event.wait()
        if self._error:
            raise self._error
        assert self._ctx is not None
        return self._ctx


async def _background_init(gate: InitGate, settings: Settings) -> None:
    """Run initialization and indexing in a background thread."""
    try:
        await asyncio.to_thread(initialize_and_index, settings)
        ctx = await asyncio.to_thread(tools.create_tool_context, settings)
        gate.set_ready(ctx)
    except Exception as e:
        gate.set_error(e)


def create_mcp_server(settings: Settings) -> FastMCP:
    """Create the MCP server.

    Registers documentation tools unconditionally.
    Registers LSP tools when CANGJIE_HOME environment variable is set.
    Initialization runs in the background via a lifespan so the server
    can accept connections immediately.

    Args:
        settings: Application settings including paths and embedding config

    Returns:
        Configured FastMCP instance with all tools registered
    """
    lsp_enabled = bool(os.environ.get("CANGJIE_HOME"))
    if not lsp_enabled:
        logger.warning("CANGJIE_HOME not set — LSP tools will not be registered.")

    gate = InitGate()

    @asynccontextmanager
    async def _server_lifespan(_server: FastMCP) -> AsyncIterator[None]:
        task = asyncio.create_task(_background_init(gate, settings))
        try:
            yield
        finally:
            task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await task

    mcp = FastMCP(
        name="cangjie_mcp",
        instructions=get_prompt(lsp_enabled=lsp_enabled),
        lifespan=_server_lifespan,
    )

    # Register documentation tools (they await the gate before processing)
    register_docs_tools(mcp, gate)

    # Register LSP tools only when CANGJIE_HOME is available
    if lsp_enabled:
        register_lsp_tools(mcp)

    return mcp
