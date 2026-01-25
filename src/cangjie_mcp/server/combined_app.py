"""Combined MCP server for Cangjie documentation and LSP.

This module creates a unified MCP server that provides both
documentation search and LSP code intelligence tools.
"""

from __future__ import annotations

from mcp.server.fastmcp import FastMCP

from cangjie_mcp.config import Settings
from cangjie_mcp.prompts import get_combined_prompt
from cangjie_mcp.server import tools
from cangjie_mcp.server.app import register_docs_tools
from cangjie_mcp.server.lsp_app import register_lsp_tools


def create_combined_mcp_server(settings: Settings) -> FastMCP:
    """Create a combined MCP server with both docs and LSP tools.

    Args:
        settings: Application settings including paths and embedding config

    Returns:
        Configured FastMCP instance with all tools registered
    """
    mcp = FastMCP(
        name="cangjie_mcp",
        instructions=get_combined_prompt(),
    )

    # Register documentation tools
    ctx = tools.create_tool_context(settings)
    register_docs_tools(mcp, ctx)

    # Register LSP tools
    register_lsp_tools(mcp)

    return mcp
