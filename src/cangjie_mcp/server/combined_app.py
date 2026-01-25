"""Combined MCP server for Cangjie documentation and LSP.

This module creates a unified MCP server that provides both
documentation search and LSP code intelligence tools.
"""

from __future__ import annotations

from mcp.server.fastmcp import FastMCP

from cangjie_mcp.config import Settings
from cangjie_mcp.server import tools
from cangjie_mcp.server.app import SERVER_INSTRUCTIONS, register_docs_tools
from cangjie_mcp.server.lsp_app import LSP_SERVER_INSTRUCTIONS, register_lsp_tools

# =============================================================================
# Combined Server Instructions
# =============================================================================

COMBINED_SERVER_INSTRUCTIONS = f"""
Cangjie MCP Server - Documentation search and code intelligence for Cangjie programming language.

This server provides two sets of tools:

## Documentation Tools (cangjie_search_docs, cangjie_get_topic, etc.)
Search and retrieve Cangjie language documentation using semantic search.

## LSP Tools (cangjie_lsp_*, requires CANGJIE_HOME)
Code intelligence features: go to definition, find references, hover, completions, etc.

---

{SERVER_INSTRUCTIONS}

---

{LSP_SERVER_INSTRUCTIONS}
""".strip()


def create_combined_mcp_server(settings: Settings) -> FastMCP:
    """Create a combined MCP server with both docs and LSP tools.

    Args:
        settings: Application settings including paths and embedding config

    Returns:
        Configured FastMCP instance with all tools registered
    """
    mcp = FastMCP(
        name="cangjie_mcp",
        instructions=COMBINED_SERVER_INSTRUCTIONS,
    )

    # Register documentation tools
    ctx = tools.create_tool_context(settings)
    register_docs_tools(mcp, ctx)

    # Register LSP tools
    register_lsp_tools(mcp)

    return mcp
