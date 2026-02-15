"""MCP tool definitions for LSP operations.

This module defines the MCP tools that expose LSP functionality
for code intelligence features. Importing this module registers
the LSP tools on the shared ``mcp`` instance from ``server.tools``.
"""

from typing import Annotated

from pydantic import Field

from cangjie_mcp.lsp import get_client, is_available
from cangjie_mcp.lsp.types import (
    CompletionItem,
    CompletionOutput,
    CompletionResult,
    DefinitionResult,
    DiagnosticOutput,
    DiagnosticsResult,
    DocumentSymbol,
    HoverOutput,
    HoverResult,
    Location,
    LocationResult,
    MarkedString,
    MarkupContent,
    ReferencesResult,
    SymbolOutput,
    SymbolsResult,
    completion_kind_name,
    severity_name,
    symbol_kind_name,
)
from cangjie_mcp.lsp.utils import uri_to_path
from cangjie_mcp.server.tools import ANNOTATIONS, mcp


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
        _check_available()
        client = get_client()
        locations = await client.definition(file_path, line - 1, character - 1)
        result_locations = [_normalize_location(loc) for loc in locations]
        return DefinitionResult(
            locations=result_locations,
            count=len(result_locations),
        )
    except Exception as e:
        return f"Error: {e}"


@mcp.tool(name="cangjie_lsp_references", annotations=ANNOTATIONS)
async def lsp_references(
    file_path: Annotated[str, Field(description="Absolute path to the .cj source file")],
    line: Annotated[int, Field(description="Line number (1-based)", ge=1)],
    character: Annotated[int, Field(description="Character position (1-based)", ge=1)],
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
        _check_available()
        client = get_client()
        locations = await client.references(file_path, line - 1, character - 1)
        result_locations = [_normalize_location(loc) for loc in locations]
        return ReferencesResult(
            locations=result_locations,
            count=len(result_locations),
        )
    except Exception as e:
        return f"Error: {e}"


@mcp.tool(name="cangjie_lsp_hover", annotations=ANNOTATIONS)
async def lsp_hover(
    file_path: Annotated[str, Field(description="Absolute path to the .cj source file")],
    line: Annotated[int, Field(description="Line number (1-based)", ge=1)],
    character: Annotated[int, Field(description="Character position (1-based)", ge=1)],
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
        _check_available()
        client = get_client()
        result = await client.hover(file_path, line - 1, character - 1)

        if not result:
            return "No hover information available"

        content = _extract_hover_content(result)

        # Extract range if available
        range_result = None
        if result.range:
            range_result = LocationResult(
                file_path=file_path,
                line=result.range.start.line + 1,
                character=result.range.start.character + 1,
                end_line=result.range.end.line + 1,
                end_character=result.range.end.character + 1,
            )

        return HoverOutput(content=content, range=range_result)
    except Exception as e:
        return f"Error: {e}"


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


@mcp.tool(name="cangjie_lsp_symbols", annotations=ANNOTATIONS)
async def lsp_symbols(
    file_path: Annotated[str, Field(description="Absolute path to the .cj source file")],
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
        _check_available()
        client = get_client()
        symbols = await client.document_symbol(file_path)
        result_symbols = [_convert_symbol(sym, file_path) for sym in symbols]
        return SymbolsResult(
            symbols=result_symbols,
            count=len(result_symbols),
        )
    except Exception as e:
        return f"Error: {e}"


@mcp.tool(name="cangjie_lsp_diagnostics", annotations=ANNOTATIONS)
async def lsp_diagnostics(
    file_path: Annotated[str, Field(description="Absolute path to the .cj source file")],
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
        _check_available()
        client = get_client()
        diagnostics = await client.get_diagnostics(file_path)

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
    except Exception as e:
        return f"Error: {e}"


@mcp.tool(name="cangjie_lsp_completion", annotations=ANNOTATIONS)
async def lsp_completion(
    file_path: Annotated[str, Field(description="Absolute path to the .cj source file")],
    line: Annotated[int, Field(description="Line number (1-based)", ge=1)],
    character: Annotated[int, Field(description="Character position (1-based)", ge=1)],
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
        _check_available()
        client = get_client()
        items = await client.completion(file_path, line - 1, character - 1)

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
    except Exception as e:
        return f"Error: {e}"


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
