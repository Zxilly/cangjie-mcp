"""LSP support module for Cangjie language.

This module provides Language Server Protocol (LSP) support for the Cangjie
programming language, enabling code intelligence features like go-to-definition,
find-references, hover information, and diagnostics.

The LSP client communicates with the Cangjie LSP server (LSPServer) bundled
with the Cangjie SDK via JSON-RPC over stdio.
"""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from cangjie_mcp.lsp.client import CangjieClient
    from cangjie_mcp.lsp.config import LSPSettings
    from cangjie_mcp.lsp.types import (
        CompletionItem,
        Diagnostic,
        DocumentSymbol,
        HoverResult,
        Location,
    )

logger = logging.getLogger(__name__)

# Global LSP client instance
_client: CangjieClient | None = None
_settings: LSPSettings | None = None


async def init(settings: LSPSettings) -> bool:
    """Initialize the LSP client with the given settings.

    Args:
        settings: LSP configuration settings

    Returns:
        True if initialization was successful, False otherwise
    """
    global _client, _settings

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
        args = settings.get_lsp_args()

        # Add require_path to PATH for C FFI and bin-dependencies
        require_path = get_resolver_require_path()
        if require_path:
            separator = get_path_separator()
            existing_path = env.get("PATH", "")
            # require_path already has trailing separator
            env["PATH"] = require_path + existing_path if existing_path else require_path.rstrip(separator)
            logger.debug(f"Added require_path to PATH: {require_path}")

        # Create and start client
        _client = CangjieClient(
            sdk_path=settings.sdk_path,
            root_path=settings.workspace_path,
            init_options=init_options,
            env=env,
            args=args,
        )

        await _client.start(timeout=settings.init_timeout)
        logger.info("LSP client initialized successfully")
        return True

    except Exception as e:
        logger.error(f"Failed to initialize LSP client: {e}")
        _client = None
        return False


async def shutdown() -> None:
    """Shutdown the LSP client."""
    global _client
    if _client is not None:
        await _client.shutdown()
        _client = None
        logger.info("LSP client shutdown complete")


def is_available() -> bool:
    """Check if the LSP client is available and initialized."""
    return _client is not None and _client.is_initialized


def get_client() -> CangjieClient:
    """Get the LSP client instance.

    Returns:
        The initialized LSP client

    Raises:
        RuntimeError: If the client is not initialized
    """
    if _client is None:
        raise RuntimeError("LSP client not initialized. Call init() first.")
    return _client


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


# LSP operation wrappers for convenience
async def definition(file_path: str, line: int, character: int) -> list[Location]:
    """Get definition locations for a symbol.

    Args:
        file_path: Absolute path to the source file
        line: Line number (0-based)
        character: Character position (0-based)

    Returns:
        List of Location models
    """
    return await get_client().definition(file_path, line, character)


async def references(file_path: str, line: int, character: int) -> list[Location]:
    """Find all references to a symbol.

    Args:
        file_path: Absolute path to the source file
        line: Line number (0-based)
        character: Character position (0-based)

    Returns:
        List of Location models
    """
    return await get_client().references(file_path, line, character)


async def hover(file_path: str, line: int, character: int) -> HoverResult | None:
    """Get hover information for a symbol.

    Args:
        file_path: Absolute path to the source file
        line: Line number (0-based)
        character: Character position (0-based)

    Returns:
        HoverResult or None
    """
    return await get_client().hover(file_path, line, character)


async def document_symbols(file_path: str) -> list[DocumentSymbol]:
    """Get document symbols.

    Args:
        file_path: Absolute path to the source file

    Returns:
        List of DocumentSymbol models
    """
    return await get_client().document_symbol(file_path)


async def diagnostics(file_path: str) -> list[Diagnostic]:
    """Get diagnostics for a file.

    Args:
        file_path: Absolute path to the source file

    Returns:
        List of Diagnostic models
    """
    return await get_client().get_diagnostics(file_path)


async def completion(file_path: str, line: int, character: int) -> list[CompletionItem]:
    """Get code completion items.

    Args:
        file_path: Absolute path to the source file
        line: Line number (0-based)
        character: Character position (0-based)

    Returns:
        List of CompletionItem models
    """
    return await get_client().completion(file_path, line, character)


__all__ = [
    "completion",
    "definition",
    "diagnostics",
    "document_symbols",
    "get_client",
    "get_settings",
    "hover",
    "init",
    "is_available",
    "references",
    "shutdown",
]
