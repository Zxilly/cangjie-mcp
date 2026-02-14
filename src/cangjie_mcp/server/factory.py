"""MCP server factory.

This module creates and configures the MCP server instance.
"""

from __future__ import annotations

from mcp.server.fastmcp import FastMCP

from cangjie_mcp.config import Settings
from cangjie_mcp.prompts import get_prompt
from cangjie_mcp.server import tools
from cangjie_mcp.server.app import register_docs_tools
from cangjie_mcp.server.lsp_app import register_lsp_tools


def create_mcp_server(settings: Settings, *, lsp_enabled: bool = True) -> FastMCP:
    """Create the MCP server.

    Args:
        settings: Application settings including paths and embedding config
        lsp_enabled: Whether to register LSP tools (requires CANGJIE_HOME)

    Returns:
        Configured FastMCP instance with all tools registered
    """
    mcp = FastMCP(
        name="cangjie_mcp",
        instructions=get_prompt(),
    )

    # Register documentation tools
    ctx = tools.create_tool_context(settings)
    register_docs_tools(mcp, ctx)

    # Register LSP tools only when CANGJIE_HOME is available
    if lsp_enabled:
        register_lsp_tools(mcp)

    return mcp
