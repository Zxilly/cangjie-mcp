"""MCP server factory.

This module creates and configures the single MCP server instance.
All tools (docs and LSP) are registered at import time.
LSP tools are lazily initialized via the lifespan context.
"""

from mcp.server.fastmcp import FastMCP

from cangjie_mcp.config import Settings


def create_mcp_server(_settings: Settings) -> FastMCP:
    """Create the MCP server.

    Returns the module-level ``mcp`` instance from ``server.tools``.
    Documentation tools are registered at import time via ``@mcp.tool``.
    LSP tools are always registered; the LSP client is lazily initialized
    during the server lifespan (only when CANGJIE_HOME is set).

    Args:
        settings: Application settings (used by lifespan via ``get_settings()``)

    Returns:
        Configured FastMCP instance with all tools registered
    """
    import cangjie_mcp.lsp.tools  # noqa: F401  # pyright: ignore[reportUnusedImport]
    from cangjie_mcp.server.tools import mcp

    return mcp
