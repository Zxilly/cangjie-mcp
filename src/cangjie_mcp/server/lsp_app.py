"""FastMCP tool registration for Cangjie LSP.

This module registers LSP tools with the MCP server.
"""

from mcp.server.fastmcp import FastMCP
from mcp.types import ToolAnnotations

from cangjie_mcp.lsp.tools import (
    lsp_completion,
    lsp_definition,
    lsp_diagnostics,
    lsp_hover,
    lsp_references,
    lsp_symbols,
    validate_file_path,
)
from cangjie_mcp.lsp.types import (
    CompletionResult,
    DefinitionResult,
    DiagnosticsResult,
    FileInput,
    HoverOutput,
    PositionInput,
    ReferencesResult,
    SymbolsResult,
)


def register_lsp_tools(mcp: FastMCP) -> None:
    """Register all LSP tools with the MCP server.

    Args:
        mcp: FastMCP server instance
    """

    @mcp.tool(
        name="cangjie_lsp_definition",
        annotations=ToolAnnotations(
            title="Go to Definition",
            readOnlyHint=True,
            destructiveHint=False,
            idempotentHint=True,
            openWorldHint=False,
        ),
    )
    async def cangjie_lsp_definition(params: PositionInput) -> DefinitionResult | str:
        """Jump to the definition of a symbol.

        Navigate to where a symbol (variable, function, class, etc.) is defined.

        Args:
            params: Position in the source file:
                - file_path (str): Absolute path to the .cj file
                - line (int): Line number (1-based)
                - character (int): Character position (1-based)

        Returns:
            DefinitionResult with locations where the symbol is defined,
            or an error message string.

        Examples:
            - Cursor on function call -> Returns function definition location
            - Cursor on variable -> Returns variable declaration location
            - Cursor on type name -> Returns type definition location
        """
        error = validate_file_path(params.file_path)
        if error:
            return error

        try:
            return await lsp_definition(params)
        except Exception as e:
            return f"Error: {e}"

    @mcp.tool(
        name="cangjie_lsp_references",
        annotations=ToolAnnotations(
            title="Find References",
            readOnlyHint=True,
            destructiveHint=False,
            idempotentHint=True,
            openWorldHint=False,
        ),
    )
    async def cangjie_lsp_references(params: PositionInput) -> ReferencesResult | str:
        """Find all references to a symbol.

        Locate all places where a symbol is used, including its definition.

        Args:
            params: Position in the source file:
                - file_path (str): Absolute path to the .cj file
                - line (int): Line number (1-based)
                - character (int): Character position (1-based)

        Returns:
            ReferencesResult with all locations where the symbol is referenced,
            or an error message string.

        Examples:
            - Find all calls to a function
            - Find all uses of a variable
            - Find all implementations of an interface
        """
        error = validate_file_path(params.file_path)
        if error:
            return error

        try:
            return await lsp_references(params)
        except Exception as e:
            return f"Error: {e}"

    @mcp.tool(
        name="cangjie_lsp_hover",
        annotations=ToolAnnotations(
            title="Get Hover Information",
            readOnlyHint=True,
            destructiveHint=False,
            idempotentHint=True,
            openWorldHint=False,
        ),
    )
    async def cangjie_lsp_hover(params: PositionInput) -> HoverOutput | str:
        """Get hover information for a symbol.

        Retrieve type information and documentation for the symbol at the cursor.

        Args:
            params: Position in the source file:
                - file_path (str): Absolute path to the .cj file
                - line (int): Line number (1-based)
                - character (int): Character position (1-based)

        Returns:
            HoverOutput with type/documentation content,
            "No hover information available", or an error message.

        Examples:
            - Hover on variable -> Shows variable type
            - Hover on function -> Shows function signature and docs
            - Hover on type -> Shows type definition
        """
        error = validate_file_path(params.file_path)
        if error:
            return error

        try:
            result = await lsp_hover(params)
            if result is None:
                return "No hover information available"
            return result
        except Exception as e:
            return f"Error: {e}"

    @mcp.tool(
        name="cangjie_lsp_symbols",
        annotations=ToolAnnotations(
            title="Get Document Symbols",
            readOnlyHint=True,
            destructiveHint=False,
            idempotentHint=True,
            openWorldHint=False,
        ),
    )
    async def cangjie_lsp_symbols(params: FileInput) -> SymbolsResult | str:
        """Get all symbols in a document.

        List all classes, functions, variables, and other symbols defined in a file.

        Args:
            params: File input:
                - file_path (str): Absolute path to the .cj file

        Returns:
            SymbolsResult with hierarchical list of symbols in the document,
            or an error message string.

        Examples:
            - Get outline of a file
            - Find all classes in a module
            - Navigate to specific functions
        """
        error = validate_file_path(params.file_path)
        if error:
            return error

        try:
            return await lsp_symbols(params)
        except Exception as e:
            return f"Error: {e}"

    @mcp.tool(
        name="cangjie_lsp_diagnostics",
        annotations=ToolAnnotations(
            title="Get Diagnostics",
            readOnlyHint=True,
            destructiveHint=False,
            idempotentHint=True,
            openWorldHint=False,
        ),
    )
    async def cangjie_lsp_diagnostics(params: FileInput) -> DiagnosticsResult | str:
        """Get diagnostics (errors and warnings) for a file.

        Retrieve all compilation errors, warnings, and hints for a source file.

        Args:
            params: File input:
                - file_path (str): Absolute path to the .cj file

        Returns:
            DiagnosticsResult with list of diagnostics and severity counts,
            or an error message string.

        Examples:
            - Check for syntax errors
            - Find type mismatches
            - Identify unused variables
        """
        error = validate_file_path(params.file_path)
        if error:
            return error

        try:
            return await lsp_diagnostics(params)
        except Exception as e:
            return f"Error: {e}"

    @mcp.tool(
        name="cangjie_lsp_completion",
        annotations=ToolAnnotations(
            title="Get Code Completion",
            readOnlyHint=True,
            destructiveHint=False,
            idempotentHint=True,
            openWorldHint=False,
        ),
    )
    async def cangjie_lsp_completion(params: PositionInput) -> CompletionResult | str:
        """Get code completion suggestions.

        Retrieve completion suggestions for the current cursor position.

        Args:
            params: Position in the source file:
                - file_path (str): Absolute path to the .cj file
                - line (int): Line number (1-based)
                - character (int): Character position (1-based)

        Returns:
            CompletionResult with list of completion items,
            or an error message string.

        Examples:
            - Complete method names after "."
            - Complete variable names
            - Complete keywords and types
        """
        error = validate_file_path(params.file_path)
        if error:
            return error

        try:
            return await lsp_completion(params)
        except Exception as e:
            return f"Error: {e}"
