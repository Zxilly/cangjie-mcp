"""MCP tool definitions for LSP operations.

This module defines the MCP tools that expose LSP functionality
for code intelligence features. Importing this module registers
the LSP tools on the shared ``mcp`` instance from ``server.tools``.

LSP tools use the same lazy-loading pattern as documentation tools:
they wait for ``lsp_ready()`` on the lifespan context before proceeding.
"""

from __future__ import annotations

import asyncio
import logging
from collections.abc import Awaitable, Callable
from typing import Annotated, Any, TypeVar

from mcp.server.fastmcp import Context
from pydantic import BaseModel, Field
from sansio_lsp_client.events import Hover as HoverEvent
from sansio_lsp_client.structs import (
    CompletionItemKind,
    DiagnosticSeverity,
    DocumentSymbol,
    Location,
    LocationLink,
    MarkedString,
    MarkupContent,
    SymbolKind,
)

from cangjie_mcp.lsp import get_server
from cangjie_mcp.lsp.utils import uri_to_path
from cangjie_mcp.server.tools import ANNOTATIONS, LifespanContext, mcp

# =============================================================================
# MCP Output Types
# =============================================================================


class LocationResult(BaseModel):
    """A normalized location result."""

    file_path: str = Field(..., description="Absolute file path")
    line: int = Field(..., description="Line number (1-based)")
    character: int = Field(..., description="Character position (1-based)")
    end_line: int | None = Field(None, description="End line number (1-based)")
    end_character: int | None = Field(None, description="End character position (1-based)")


class DefinitionResult(BaseModel):
    """Result of a definition request."""

    locations: list[LocationResult] = Field(default_factory=lambda: list[LocationResult]())
    count: int = 0


class ReferencesResult(BaseModel):
    """Result of a references request."""

    locations: list[LocationResult] = Field(default_factory=lambda: list[LocationResult]())
    count: int = 0


class HoverOutput(BaseModel):
    """Result of a hover request for MCP."""

    content: str = Field(..., description="Hover content (markdown)")
    range: LocationResult | None = None


class SymbolOutput(BaseModel):
    """A symbol in MCP output format."""

    name: str
    kind: str
    line: int
    character: int
    end_line: int
    end_character: int
    children: list[SymbolOutput] | None = None


SymbolOutput.model_rebuild()


class SymbolsResult(BaseModel):
    """Result of a document symbols request."""

    symbols: list[SymbolOutput] = Field(default_factory=lambda: list[SymbolOutput]())
    count: int = 0


class DiagnosticOutput(BaseModel):
    """A diagnostic in MCP output format."""

    message: str
    severity: str
    line: int
    character: int
    end_line: int
    end_character: int
    code: str | None = None
    source: str | None = None


class DiagnosticsResult(BaseModel):
    """Result of a diagnostics request."""

    diagnostics: list[DiagnosticOutput] = Field(default_factory=lambda: list[DiagnosticOutput]())
    error_count: int = 0
    warning_count: int = 0
    info_count: int = 0
    hint_count: int = 0


class CompletionOutput(BaseModel):
    """A completion item in MCP output format."""

    label: str
    kind: str | None = None
    detail: str | None = None
    documentation: str | None = None
    insert_text: str | None = None


class CompletionResult(BaseModel):
    """Result of a completion request."""

    items: list[CompletionOutput] = Field(default_factory=lambda: list[CompletionOutput]())
    count: int = 0


# =============================================================================
# Helper Functions
# =============================================================================


def symbol_kind_name(kind: int) -> str:
    """Convert symbol kind integer to string name."""
    try:
        name = SymbolKind(kind).name.lower()
        if name == "enummember":
            return "enum member"
        if name == "typeparameter":
            return "type parameter"
        return name
    except ValueError:
        return "unknown"


def completion_kind_name(kind: int | CompletionItemKind | None) -> str | None:
    """Convert completion item kind to string name."""
    if kind is None:
        return None
    try:
        name = CompletionItemKind(kind).name.lower()
        if name == "enummember":
            return "enum member"
        if name == "typeparameter":
            return "type parameter"
        return name
    except ValueError:
        return "unknown"


def severity_name(severity: int | DiagnosticSeverity | None) -> str:
    """Convert diagnostic severity to string name."""
    if severity is None:
        return "unknown"
    try:
        return DiagnosticSeverity(severity).name.lower()
    except ValueError:
        return "unknown"


# =============================================================================
# Internal Helpers
# =============================================================================


def _normalize_location(loc: Location | LocationLink) -> LocationResult:
    """Convert LSP Location or LocationLink to MCP format.

    Args:
        loc: LSP Location or LocationLink model

    Returns:
        LocationResult in 1-based format
    """
    if isinstance(loc, LocationLink):
        return LocationResult(
            file_path=str(uri_to_path(loc.targetUri)),
            line=loc.targetRange.start.line + 1,
            character=loc.targetRange.start.character + 1,
            end_line=loc.targetRange.end.line + 1,
            end_character=loc.targetRange.end.character + 1,
        )
    return LocationResult(
        file_path=str(uri_to_path(loc.uri)),
        line=loc.range.start.line + 1,
        character=loc.range.start.character + 1,
        end_line=loc.range.end.line + 1,
        end_character=loc.range.end.character + 1,
    )


_LSP_UNAVAILABLE_MSG = "LSP is not available. Ensure CANGJIE_HOME is set and the LSP server can start."
_LSP_TIMEOUT_MSG = "LSP request timed out after 10 seconds. The LSP server may be unresponsive."
_LSP_CRASHED_MSG = "LSP server process has stopped unexpectedly. Restart the MCP server to recover."
_LSP_TIMEOUT = 10.0

logger = logging.getLogger(__name__)

_T = TypeVar("_T")


async def _lsp_call(  # noqa: UP047
    ctx: Context[Any, LifespanContext, Any],
    fn: Callable[[], Awaitable[_T]],
) -> _T | str:
    """Execute an LSP operation with liveness check and timeout.

    Args:
        ctx: MCP context with lifespan
        fn: Async callable that performs the LSP operation

    Returns:
        The result of fn, or an error message string
    """
    if not await ctx.request_context.lifespan_context.lsp_ready():
        return _LSP_UNAVAILABLE_MSG

    server = get_server()
    if not server.is_alive:
        logger.error("LSP server process is not running")
        return _LSP_CRASHED_MSG

    try:
        return await asyncio.wait_for(fn(), timeout=_LSP_TIMEOUT)
    except TimeoutError:
        logger.warning("LSP request timed out")
        return _LSP_TIMEOUT_MSG


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
    from pathlib import Path

    path = Path(file_path)

    if not path.exists():
        return f"File not found: {file_path}"

    if not path.is_file():
        return f"Not a file: {file_path}"

    if path.suffix != ".cj":
        return f"Not a Cangjie file (expected .cj extension): {file_path}"

    return None


# =============================================================================
# Tool Implementations
# =============================================================================


@mcp.tool(name="cangjie_lsp_definition", annotations=ANNOTATIONS)
async def lsp_definition(
    file_path: Annotated[str, Field(description="Absolute path to the .cj source file")],
    line: Annotated[int, Field(description="Line number (1-based)", ge=1)],
    character: Annotated[int, Field(description="Character position (1-based)", ge=1)],
    *,
    ctx: Context[Any, LifespanContext, Any],
) -> DefinitionResult | str:
    """Jump to the definition of a symbol.

    Navigate to where a symbol (variable, function, class, etc.) is defined.

    Args:
        file_path: Absolute path to the .cj file
        line: Line number (1-based)
        character: Character position (1-based)

    Returns:
        DefinitionResult with locations where the symbol is defined,
        or an error message string.

    Examples:
        - Cursor on function call -> Returns function definition location
        - Cursor on variable -> Returns variable declaration location
        - Cursor on type name -> Returns type definition location
    """
    error = validate_file_path(file_path)
    if error:
        return error
    try:

        async def _call() -> DefinitionResult:
            server = get_server()
            event = await server.definition(file_path, line - 1, character - 1)
            locations: list[LocationResult] = []
            result = event.result
            if result is not None:
                if isinstance(result, list):
                    locations = [_normalize_location(loc) for loc in result]
                else:
                    locations = [_normalize_location(result)]
            return DefinitionResult(locations=locations, count=len(locations))

        return await _lsp_call(ctx, _call)
    except Exception as e:
        return f"Error: {e}"


@mcp.tool(name="cangjie_lsp_references", annotations=ANNOTATIONS)
async def lsp_references(
    file_path: Annotated[str, Field(description="Absolute path to the .cj source file")],
    line: Annotated[int, Field(description="Line number (1-based)", ge=1)],
    character: Annotated[int, Field(description="Character position (1-based)", ge=1)],
    *,
    ctx: Context[Any, LifespanContext, Any],
) -> ReferencesResult | str:
    """Find all references to a symbol.

    Locate all places where a symbol is used, including its definition.

    Args:
        file_path: Absolute path to the .cj file
        line: Line number (1-based)
        character: Character position (1-based)

    Returns:
        ReferencesResult with all locations where the symbol is referenced,
        or an error message string.

    Examples:
        - Find all calls to a function
        - Find all uses of a variable
        - Find all implementations of an interface
    """
    error = validate_file_path(file_path)
    if error:
        return error
    try:

        async def _call() -> ReferencesResult:
            server = get_server()
            event = await server.references(file_path, line - 1, character - 1)
            locations: list[LocationResult] = []
            if event.result:
                locations = [_normalize_location(loc) for loc in event.result]
            return ReferencesResult(locations=locations, count=len(locations))

        return await _lsp_call(ctx, _call)
    except Exception as e:
        return f"Error: {e}"


@mcp.tool(name="cangjie_lsp_hover", annotations=ANNOTATIONS)
async def lsp_hover(
    file_path: Annotated[str, Field(description="Absolute path to the .cj source file")],
    line: Annotated[int, Field(description="Line number (1-based)", ge=1)],
    character: Annotated[int, Field(description="Character position (1-based)", ge=1)],
    *,
    ctx: Context[Any, LifespanContext, Any],
) -> HoverOutput | str:
    """Get hover information for a symbol.

    Retrieve type information and documentation for the symbol at the cursor.

    Args:
        file_path: Absolute path to the .cj file
        line: Line number (1-based)
        character: Character position (1-based)

    Returns:
        HoverOutput with type/documentation content,
        "No hover information available", or an error message.

    Examples:
        - Hover on variable -> Shows variable type
        - Hover on function -> Shows function signature and docs
        - Hover on type -> Shows type definition
    """
    error = validate_file_path(file_path)
    if error:
        return error
    try:

        async def _call() -> HoverOutput | str:
            server = get_server()
            event = await server.hover(file_path, line - 1, character - 1)
            if not event.contents:
                return "No hover information available"
            content = _extract_hover_content(event)
            range_result = None
            if event.range:
                range_result = LocationResult(
                    file_path=file_path,
                    line=event.range.start.line + 1,
                    character=event.range.start.character + 1,
                    end_line=event.range.end.line + 1,
                    end_character=event.range.end.character + 1,
                )
            return HoverOutput(content=content, range=range_result)

        return await _lsp_call(ctx, _call)
    except Exception as e:
        return f"Error: {e}"


def _extract_hover_content(event: HoverEvent) -> str:
    """Extract display content from a Hover event.

    Args:
        event: Hover event from sansio-lsp-client

    Returns:
        Extracted content string
    """
    contents = event.contents
    if isinstance(contents, (MarkupContent, MarkedString)):
        return contents.value
    if isinstance(contents, str):
        return contents
    # list[MarkedString | str]
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
        sym: DocumentSymbol model from sansio-lsp-client
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


@mcp.tool(name="cangjie_lsp_symbols", annotations=ANNOTATIONS)
async def lsp_symbols(
    file_path: Annotated[str, Field(description="Absolute path to the .cj source file")],
    *,
    ctx: Context[Any, LifespanContext, Any],
) -> SymbolsResult | str:
    """Get all symbols in a document.

    List all classes, functions, variables, and other symbols defined in a file.

    Args:
        file_path: Absolute path to the .cj file

    Returns:
        SymbolsResult with hierarchical list of symbols in the document,
        or an error message string.

    Examples:
        - Get outline of a file
        - Find all classes in a module
        - Navigate to specific functions
    """
    error = validate_file_path(file_path)
    if error:
        return error
    try:

        async def _call() -> SymbolsResult:
            server = get_server()
            event = await server.document_symbol(file_path)
            result_symbols: list[SymbolOutput] = []
            if event.result:
                for sym in event.result:
                    if isinstance(sym, DocumentSymbol):
                        result_symbols.append(_convert_symbol(sym, file_path))
            return SymbolsResult(symbols=result_symbols, count=len(result_symbols))

        return await _lsp_call(ctx, _call)
    except Exception as e:
        return f"Error: {e}"


@mcp.tool(name="cangjie_lsp_diagnostics", annotations=ANNOTATIONS)
async def lsp_diagnostics(
    file_path: Annotated[str, Field(description="Absolute path to the .cj source file")],
    *,
    ctx: Context[Any, LifespanContext, Any],
) -> DiagnosticsResult | str:
    """Get diagnostics (errors and warnings) for a file.

    Retrieve all compilation errors, warnings, and hints for a source file.

    Args:
        file_path: Absolute path to the .cj file

    Returns:
        DiagnosticsResult with list of diagnostics and severity counts,
        or an error message string.

    Examples:
        - Check for syntax errors
        - Find type mismatches
        - Identify unused variables
    """
    error = validate_file_path(file_path)
    if error:
        return error
    try:

        async def _call() -> DiagnosticsResult:
            server = get_server()
            diag_list = await server.get_diagnostics(file_path)
            result_diagnostics: list[DiagnosticOutput] = []
            error_count = 0
            warning_count = 0
            info_count = 0
            hint_count = 0
            for diag in diag_list:
                severity_str = severity_name(diag.severity)
                if diag.severity == DiagnosticSeverity.ERROR:
                    error_count += 1
                elif diag.severity == DiagnosticSeverity.WARNING:
                    warning_count += 1
                elif diag.severity == DiagnosticSeverity.INFORMATION:
                    info_count += 1
                elif diag.severity == DiagnosticSeverity.HINT:
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

        return await _lsp_call(ctx, _call)
    except Exception as e:
        return f"Error: {e}"


@mcp.tool(name="cangjie_lsp_completion", annotations=ANNOTATIONS)
async def lsp_completion(
    file_path: Annotated[str, Field(description="Absolute path to the .cj source file")],
    line: Annotated[int, Field(description="Line number (1-based)", ge=1)],
    character: Annotated[int, Field(description="Character position (1-based)", ge=1)],
    *,
    ctx: Context[Any, LifespanContext, Any],
) -> CompletionResult | str:
    """Get code completion suggestions.

    Retrieve completion suggestions for the current cursor position.

    Args:
        file_path: Absolute path to the .cj file
        line: Line number (1-based)
        character: Character position (1-based)

    Returns:
        CompletionResult with list of completion items,
        or an error message string.

    Examples:
        - Complete method names after "."
        - Complete variable names
        - Complete keywords and types
    """
    error = validate_file_path(file_path)
    if error:
        return error
    try:

        async def _call() -> CompletionResult:
            server = get_server()
            event = await server.completion(file_path, line - 1, character - 1)
            result_items: list[CompletionOutput] = []
            if event.completion_list:
                for item in event.completion_list.items:
                    doc_str = _extract_documentation(item)
                    result_items.append(
                        CompletionOutput(
                            label=item.label,
                            kind=completion_kind_name(item.kind),
                            detail=item.detail,
                            documentation=doc_str,
                            insert_text=item.insertText,
                        )
                    )
            return CompletionResult(items=result_items, count=len(result_items))

        return await _lsp_call(ctx, _call)
    except Exception as e:
        return f"Error: {e}"


def _extract_documentation(item: Any) -> str | None:  # noqa: ANN401
    """Extract documentation string from a CompletionItem.

    Args:
        item: CompletionItem from sansio-lsp-client

    Returns:
        Documentation string or None
    """
    doc = item.documentation
    if isinstance(doc, str):
        return doc
    if isinstance(doc, MarkupContent):
        return doc.value
    return None
