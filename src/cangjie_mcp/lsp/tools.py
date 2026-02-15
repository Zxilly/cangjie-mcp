"""MCP tool definitions for LSP operations.

This module defines the MCP tools that expose LSP functionality
for code intelligence features.
"""

from __future__ import annotations

from pathlib import Path

from cangjie_mcp.lsp import get_client, is_available
from cangjie_mcp.lsp.types import (
    CompletionItem,
    CompletionOutput,
    CompletionResult,
    DefinitionResult,
    DiagnosticOutput,
    DiagnosticsResult,
    DocumentSymbol,
    FileInput,
    HoverOutput,
    HoverResult,
    Location,
    LocationResult,
    MarkedString,
    MarkupContent,
    PositionInput,
    ReferencesResult,
    SymbolOutput,
    SymbolsResult,
    completion_kind_name,
    severity_name,
    symbol_kind_name,
)
from cangjie_mcp.lsp.utils import uri_to_path


def _normalize_location(loc: Location) -> LocationResult:
    """Convert LSP Location to MCP format.

    Args:
        loc: LSP Location model

    Returns:
        LocationResult in 1-based format
    """
    return LocationResult(
        file_path=str(uri_to_path(loc.uri)),
        line=loc.range.start.line + 1,
        character=loc.range.start.character + 1,
        end_line=loc.range.end.line + 1,
        end_character=loc.range.end.character + 1,
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

    content = _extract_hover_content(result)

    # Extract range if available
    range_result = None
    if result.range:
        range_result = LocationResult(
            file_path=params.file_path,
            line=result.range.start.line + 1,
            character=result.range.start.character + 1,
            end_line=result.range.end.line + 1,
            end_character=result.range.end.character + 1,
        )

    return HoverOutput(content=content, range=range_result)


def _extract_hover_content(result: HoverResult) -> str:
    """Extract display content from a HoverResult.

    Args:
        result: HoverResult model

    Returns:
        Extracted content string
    """
    contents = result.contents
    if isinstance(contents, (MarkupContent, MarkedString)):
        return contents.value
    if isinstance(contents, str):
        return contents
    # list[MarkupContent | MarkedString | str]
    parts: list[str] = []
    for item in contents:
        if isinstance(item, (MarkupContent, MarkedString)):
            parts.append(item.value)
        else:
            parts.append(item)
    return "\n\n".join(parts)


def _convert_symbol(sym: DocumentSymbol, file_path: str) -> SymbolOutput:
    """Convert LSP DocumentSymbol to MCP format.

    Args:
        sym: DocumentSymbol model
        file_path: Source file path

    Returns:
        SymbolOutput in 1-based format
    """
    children = None
    if sym.children:
        children = [_convert_symbol(child, file_path) for child in sym.children]

    return SymbolOutput(
        name=sym.name,
        kind=symbol_kind_name(sym.kind),
        line=sym.range.start.line + 1,
        character=sym.range.start.character + 1,
        end_line=sym.range.end.line + 1,
        end_character=sym.range.end.character + 1,
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
        severity_str = severity_name(diag.severity)

        # Count by severity
        if diag.severity == 1:
            error_count += 1
        elif diag.severity == 2:
            warning_count += 1
        elif diag.severity == 3:
            info_count += 1
        elif diag.severity == 4:
            hint_count += 1

        code_str = str(diag.code) if diag.code is not None else None

        result_diagnostics.append(
            DiagnosticOutput(
                message=diag.message,
                severity=severity_str,
                line=diag.range.start.line + 1,
                character=diag.range.start.character + 1,
                end_line=diag.range.end.line + 1,
                end_character=diag.range.end.character + 1,
                code=code_str,
                source=diag.source,
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
        doc_str = _extract_documentation(item)

        result_items.append(
            CompletionOutput(
                label=item.label,
                kind=completion_kind_name(item.kind),
                detail=item.detail,
                documentation=doc_str,
                insert_text=item.insert_text,
            )
        )

    return CompletionResult(
        items=result_items,
        count=len(result_items),
    )


def _extract_documentation(item: CompletionItem) -> str | None:
    """Extract documentation string from a CompletionItem.

    Args:
        item: CompletionItem model

    Returns:
        Documentation string or None
    """
    doc = item.documentation
    if isinstance(doc, str):
        return doc
    if isinstance(doc, MarkupContent):
        return doc.value
    return None


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
