"""MCP server factory.

This module creates and configures the single MCP server instance.
LSP tools are conditionally registered when CANGJIE_HOME is set.
"""

import os

from mcp.server.fastmcp import FastMCP

from cangjie_mcp.config import Settings
from cangjie_mcp.utils import logger


def create_mcp_server(_settings: Settings) -> FastMCP:
    """Create the MCP server.

    Returns the module-level ``mcp`` instance from ``server.tools``.
    Documentation tools are registered at import time via ``@mcp.tool``.
    LSP tools are conditionally registered when CANGJIE_HOME is set
    (importing the ``lsp.tools`` module triggers ``@mcp.tool`` registration).

    Args:
        settings: Application settings (used by lifespan via ``get_settings()``)

    Returns:
        Configured FastMCP instance with all tools registered
    """
    from cangjie_mcp.server.tools import mcp

    if bool(os.environ.get("CANGJIE_HOME")):
        import cangjie_mcp.lsp.tools  # noqa: F401  # pyright: ignore[reportUnusedImport]
    else:
        logger.warning("CANGJIE_HOME not set — LSP tools will not be registered.")

    return mcp
