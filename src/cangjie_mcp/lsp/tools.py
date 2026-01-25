"""MCP tool definitions for LSP operations.

This module defines the MCP tools that expose LSP functionality
for code intelligence features.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from cangjie_mcp.lsp import get_client, is_available
from cangjie_mcp.lsp.types import (
    CompletionOutput,
    CompletionResult,
    DefinitionResult,
    DiagnosticOutput,
    DiagnosticsResult,
    FileInput,
    HoverOutput,
    LocationResult,
    PositionInput,
    ReferencesResult,
    SymbolOutput,
    SymbolsResult,
    completion_kind_name,
    severity_name,
    symbol_kind_name,
)
from cangjie_mcp.lsp.utils import uri_to_path


def _normalize_location(loc: dict[str, Any]) -> LocationResult:
    """Convert LSP location to MCP format.

    Args:
        loc: LSP location dictionary

    Returns:
        LocationResult in 1-based format
    """
    uri = loc.get("uri", "")
    range_data = loc.get("range", {})
    start = range_data.get("start", {})
    end = range_data.get("end", {})

    return LocationResult(
        file_path=str(uri_to_path(uri)),
        line=start.get("line", 0) + 1,
        character=start.get("character", 0) + 1,
        end_line=end.get("line", 0) + 1 if end else None,
        end_character=end.get("character", 0) + 1 if end else None,
    )


def _check_available() -> None:
    """Check if LSP client is available."""
    if not is_available():
        raise RuntimeError("LSP client not initialized. Please ensure the LSP server is running.")


# =============================================================================
# Tool Implementations
# =============================================================================


async def lsp_definition(params: PositionInput) -> DefinitionResult:
    """Get definition locations for a symbol.

    Args:
        params: Position input with file_path, line, and character (1-based)

    Returns:
        DefinitionResult with locations
    """
    _check_available()
    client = get_client()

    locations = await client.definition(
        params.file_path,
        params.line_0based,
        params.character_0based,
    )

    result_locations = [_normalize_location(loc) for loc in locations]

    return DefinitionResult(
        locations=result_locations,
        count=len(result_locations),
    )


async def lsp_references(params: PositionInput) -> ReferencesResult:
    """Find all references to a symbol.

    Args:
        params: Position input with file_path, line, and character (1-based)

    Returns:
        ReferencesResult with locations
    """
    _check_available()
    client = get_client()

    locations = await client.references(
        params.file_path,
        params.line_0based,
        params.character_0based,
    )

    result_locations = [_normalize_location(loc) for loc in locations]

    return ReferencesResult(
        locations=result_locations,
        count=len(result_locations),
    )


async def lsp_hover(params: PositionInput) -> HoverOutput | None:
    """Get hover information for a symbol.

    Args:
        params: Position input with file_path, line, and character (1-based)

    Returns:
        HoverOutput with content, or None if no hover info available
    """
    _check_available()
    client = get_client()

    result = await client.hover(
        params.file_path,
        params.line_0based,
        params.character_0based,
    )

    if not result:
        return None

    # Extract content
    contents = result.get("contents", "")
    if isinstance(contents, dict):
        content = contents.get("value", str(contents))
    elif isinstance(contents, list):
        parts = []
        for item in contents:
            if isinstance(item, str):
                parts.append(item)
            elif isinstance(item, dict):
                parts.append(item.get("value", str(item)))
        content = "\n\n".join(parts)
    else:
        content = str(contents)

    # Extract range if available
    range_data = result.get("range")
    range_result = None
    if range_data:
        start = range_data.get("start", {})
        end = range_data.get("end", {})
        range_result = LocationResult(
            file_path=params.file_path,
            line=start.get("line", 0) + 1,
            character=start.get("character", 0) + 1,
            end_line=end.get("line", 0) + 1,
            end_character=end.get("character", 0) + 1,
        )

    return HoverOutput(content=content, range=range_result)


def _convert_symbol(sym: dict[str, Any], file_path: str) -> SymbolOutput:
    """Convert LSP symbol to MCP format.

    Args:
        sym: LSP symbol dictionary
        file_path: Source file path

    Returns:
        SymbolOutput in 1-based format
    """
    range_data = sym.get("range", sym.get("location", {}).get("range", {}))
    start = range_data.get("start", {})
    end = range_data.get("end", {})

    children = None
    if sym.get("children"):
        children = [_convert_symbol(child, file_path) for child in sym["children"]]

    return SymbolOutput(
        name=sym.get("name", ""),
        kind=symbol_kind_name(sym.get("kind", 0)),
        line=start.get("line", 0) + 1,
        character=start.get("character", 0) + 1,
        end_line=end.get("line", 0) + 1,
        end_character=end.get("character", 0) + 1,
        children=children,
    )


async def lsp_symbols(params: FileInput) -> SymbolsResult:
    """Get document symbols.

    Args:
        params: File input with file_path

    Returns:
        SymbolsResult with symbols
    """
    _check_available()
    client = get_client()

    symbols = await client.document_symbol(params.file_path)

    result_symbols = [_convert_symbol(sym, params.file_path) for sym in symbols]

    return SymbolsResult(
        symbols=result_symbols,
        count=len(result_symbols),
    )


async def lsp_diagnostics(params: FileInput) -> DiagnosticsResult:
    """Get diagnostics for a file.

    Args:
        params: File input with file_path

    Returns:
        DiagnosticsResult with diagnostics and counts
    """
    _check_available()
    client = get_client()

    diagnostics = await client.get_diagnostics(params.file_path)

    result_diagnostics: list[DiagnosticOutput] = []
    error_count = 0
    warning_count = 0
    info_count = 0
    hint_count = 0

    for diag in diagnostics:
        range_data = diag.get("range", {})
        start = range_data.get("start", {})
        end = range_data.get("end", {})
        severity = diag.get("severity")
        severity_str = severity_name(severity)

        # Count by severity
        if severity == 1:
            error_count += 1
        elif severity == 2:
            warning_count += 1
        elif severity == 3:
            info_count += 1
        elif severity == 4:
            hint_count += 1

        code = diag.get("code")
        code_str = str(code) if code is not None else None

        result_diagnostics.append(
            DiagnosticOutput(
                message=diag.get("message", ""),
                severity=severity_str,
                line=start.get("line", 0) + 1,
                character=start.get("character", 0) + 1,
                end_line=end.get("line", 0) + 1,
                end_character=end.get("character", 0) + 1,
                code=code_str,
                source=diag.get("source"),
            )
        )

    return DiagnosticsResult(
        diagnostics=result_diagnostics,
        error_count=error_count,
        warning_count=warning_count,
        info_count=info_count,
        hint_count=hint_count,
    )


async def lsp_completion(params: PositionInput) -> CompletionResult:
    """Get code completion items.

    Args:
        params: Position input with file_path, line, and character (1-based)

    Returns:
        CompletionResult with completion items
    """
    _check_available()
    client = get_client()

    items = await client.completion(
        params.file_path,
        params.line_0based,
        params.character_0based,
    )

    result_items: list[CompletionOutput] = []

    for item in items:
        # Extract documentation
        doc = item.get("documentation")
        doc_str = None
        if isinstance(doc, str):
            doc_str = doc
        elif isinstance(doc, dict):
            doc_str = doc.get("value", str(doc))

        result_items.append(
            CompletionOutput(
                label=item.get("label", ""),
                kind=completion_kind_name(item.get("kind")),
                detail=item.get("detail"),
                documentation=doc_str,
                insert_text=item.get("insertText"),
            )
        )

    return CompletionResult(
        items=result_items,
        count=len(result_items),
    )


# =============================================================================
# Validation helpers
# =============================================================================


def validate_file_path(file_path: str) -> str | None:
    """Validate that a file path exists and is a Cangjie file.

    Args:
        file_path: Path to validate

    Returns:
        Error message if invalid, None if valid
    """
    path = Path(file_path)

    if not path.exists():
        return f"File not found: {file_path}"

    if not path.is_file():
        return f"Not a file: {file_path}"

    if path.suffix != ".cj":
        return f"Not a Cangjie file (expected .cj extension): {file_path}"

    return None
