"""MCP server factory.

This module creates and configures the single MCP server instance.
LSP tools are conditionally registered when CANGJIE_HOME is set.
"""

from __future__ import annotations

import os

from mcp.server.fastmcp import FastMCP

from cangjie_mcp.config import Settings
from cangjie_mcp.prompts import get_prompt
from cangjie_mcp.server import tools
from cangjie_mcp.server.app import register_docs_tools
from cangjie_mcp.server.lsp_app import register_lsp_tools
from cangjie_mcp.utils import console


def create_mcp_server(settings: Settings) -> FastMCP:
    """Create the MCP server.

    Registers documentation tools unconditionally.
    Registers LSP tools when CANGJIE_HOME environment variable is set.

    Args:
        settings: Application settings including paths and embedding config

    Returns:
        Configured FastMCP instance with all tools registered
    """
    lsp_enabled = bool(os.environ.get("CANGJIE_HOME"))
    if not lsp_enabled:
        console.print("[yellow]CANGJIE_HOME not set — LSP tools will not be registered.[/yellow]")

    mcp = FastMCP(
        name="cangjie_mcp",
        instructions=get_prompt(lsp_enabled=lsp_enabled),
    )

    # Register documentation tools
    ctx = tools.create_tool_context(settings)
    register_docs_tools(mcp, ctx)

    # Register LSP tools only when CANGJIE_HOME is available
    if lsp_enabled:
        register_lsp_tools(mcp)

    return mcp
