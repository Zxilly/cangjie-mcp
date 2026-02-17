"""LSP support module for Cangjie language.

This module provides Language Server Protocol (LSP) support for the Cangjie
programming language, enabling code intelligence features like go-to-definition,
find-references, hover information, and diagnostics.

The LSP client communicates with the Cangjie LSP server (LSPServer) bundled
with the Cangjie SDK, using sansio-lsp-client for protocol handling and
asyncio subprocess for I/O.
"""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from cangjie_mcp.lsp.client import CangjieClient
    from cangjie_mcp.lsp.config import LSPSettings

logger = logging.getLogger(__name__)

# Global LSP client instance
_server: CangjieClient | None = None
_settings: LSPSettings | None = None


async def init(settings: LSPSettings) -> bool:
    """Initialize the LSP client with the given settings.

    Args:
        settings: LSP configuration settings

    Returns:
        True if initialization was successful, False otherwise
    """
    global _server, _settings

    from cangjie_mcp.lsp.client import CangjieClient
    from cangjie_mcp.lsp.config import (
        build_init_options,
        get_platform_env,
        get_resolver_require_path,
    )
    from cangjie_mcp.lsp.utils import get_path_separator

    try:
        _settings = settings

        # Build environment and initialization options
        env = get_platform_env(settings.sdk_path)
        init_options = build_init_options(settings)

        # Add require_path to PATH for C FFI and bin-dependencies
        require_path = get_resolver_require_path()
        if require_path:
            separator = get_path_separator()
            existing_path = env.get("PATH", "")
            # require_path already has trailing separator
            env["PATH"] = require_path + existing_path if existing_path else require_path.rstrip(separator)
            logger.debug("Added require_path to PATH: %s", require_path)

        # Create and start client
        _server = CangjieClient(
            settings=settings,
            init_options=init_options,
            env=env,
        )

        await _server.start(timeout=settings.init_timeout)
        logger.info("LSP client initialized successfully")
        return True

    except Exception:
        logger.exception("Failed to initialize LSP client")
        if _server is not None:
            await _server.shutdown()
        _server = None
        return False


async def shutdown() -> None:
    """Shutdown the LSP client."""
    global _server
    if _server is not None:
        await _server.shutdown()
        _server = None
        logger.info("LSP client shutdown complete")


def is_available() -> bool:
    """Check if the LSP client is available and initialized."""
    return _server is not None and _server.is_initialized


def get_server() -> CangjieClient:
    """Get the LSP client instance.

    Returns:
        The initialized LSP client

    Raises:
        RuntimeError: If the client is not initialized
    """
    if _server is None:
        raise RuntimeError("LSP client not initialized. Call init() first.")
    return _server


def get_settings() -> LSPSettings:
    """Get the LSP settings.

    Returns:
        The LSP settings

    Raises:
        RuntimeError: If settings are not initialized
    """
    if _settings is None:
        raise RuntimeError("LSP settings not initialized. Call init() first.")
    return _settings


__all__ = [
    "get_server",
    "get_settings",
    "init",
    "is_available",
    "shutdown",
]
