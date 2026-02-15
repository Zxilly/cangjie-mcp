"""Type definitions for LSP module.

This module contains Pydantic models for LSP tool inputs and outputs.
"""

from __future__ import annotations

from enum import IntEnum
from typing import Any

from pydantic import BaseModel, Field

# =============================================================================
# LSP Position and Range Types
# =============================================================================


class Position(BaseModel):
    """A position in a text document (0-based)."""

    line: int = Field(..., description="Line number (0-based)")
    character: int = Field(..., description="Character offset (0-based)")


class Range(BaseModel):
    """A range in a text document."""

    start: Position
    end: Position


class Location(BaseModel):
    """A location in a document."""

    uri: str = Field(..., description="Document URI")
    range: Range


# =============================================================================
# LSP Symbol Types
# =============================================================================


class SymbolKind(IntEnum):
    """Symbol kinds as defined by LSP."""

    FILE = 1
    MODULE = 2
    NAMESPACE = 3
    PACKAGE = 4
    CLASS = 5
    METHOD = 6
    PROPERTY = 7
    FIELD = 8
    CONSTRUCTOR = 9
    ENUM = 10
    INTERFACE = 11
    FUNCTION = 12
    VARIABLE = 13
    CONSTANT = 14
    STRING = 15
    NUMBER = 16
    BOOLEAN = 17
    ARRAY = 18
    OBJECT = 19
    KEY = 20
    NULL = 21
    ENUM_MEMBER = 22
    STRUCT = 23
    EVENT = 24
    OPERATOR = 25
    TYPE_PARAMETER = 26


class DocumentSymbol(BaseModel):
    """A symbol in a document."""

    name: str
    kind: int
    range: Range
    selection_range: Range = Field(..., alias="selectionRange")
    children: list[DocumentSymbol] | None = None

    model_config = {"populate_by_name": True}


# =============================================================================
# LSP Diagnostic Types
# =============================================================================


class DiagnosticSeverity(IntEnum):
    """Diagnostic severity levels."""

    ERROR = 1
    WARNING = 2
    INFORMATION = 3
    HINT = 4


class Diagnostic(BaseModel):
    """A diagnostic message."""

    range: Range
    severity: int | None = None
    code: str | int | None = None
    source: str | None = None
    message: str
    related_information: list[dict[str, Any]] | None = Field(None, alias="relatedInformation")

    model_config = {"populate_by_name": True}


# =============================================================================
# LSP Completion Types
# =============================================================================


class CompletionItemKind(IntEnum):
    """Completion item kinds."""

    TEXT = 1
    METHOD = 2
    FUNCTION = 3
    CONSTRUCTOR = 4
    FIELD = 5
    VARIABLE = 6
    CLASS = 7
    INTERFACE = 8
    MODULE = 9
    PROPERTY = 10
    UNIT = 11
    VALUE = 12
    ENUM = 13
    KEYWORD = 14
    SNIPPET = 15
    COLOR = 16
    FILE = 17
    REFERENCE = 18
    FOLDER = 19
    ENUM_MEMBER = 20
    CONSTANT = 21
    STRUCT = 22
    EVENT = 23
    OPERATOR = 24
    TYPE_PARAMETER = 25


class CompletionItem(BaseModel):
    """A completion item."""

    label: str
    kind: int | None = None
    detail: str | None = None
    documentation: str | MarkupContent | None = None
    insert_text: str | None = Field(None, alias="insertText")
    insert_text_format: int | None = Field(None, alias="insertTextFormat")

    model_config = {"populate_by_name": True}


# =============================================================================
# LSP Hover Types
# =============================================================================


class MarkupContent(BaseModel):
    """LSP MarkupContent: { kind: MarkupKind, value: string }."""

    kind: str = "markdown"
    value: str


class MarkedString(BaseModel):
    """LSP MarkedString (object form): { language: string, value: string }."""

    language: str
    value: str


class HoverResult(BaseModel):
    """Result of a hover request."""

    contents: MarkupContent | MarkedString | str | list[MarkupContent | MarkedString | str]
    range: Range | None = None


# =============================================================================
# MCP Tool Input Types
# =============================================================================


class PositionInput(BaseModel):
    """Input for position-based LSP operations."""

    file_path: str = Field(..., description="Absolute path to the source file")
    line: int = Field(..., ge=1, description="Line number (1-based)")
    character: int = Field(..., ge=1, description="Character position (1-based)")

    @property
    def line_0based(self) -> int:
        """Get 0-based line number for LSP."""
        return self.line - 1

    @property
    def character_0based(self) -> int:
        """Get 0-based character position for LSP."""
        return self.character - 1


class FileInput(BaseModel):
    """Input for file-based LSP operations."""

    file_path: str = Field(..., description="Absolute path to the source file")


# =============================================================================
# MCP Tool Output Types
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
        return SymbolKind(kind).name.lower().replace("_", " ")
    except ValueError:
        return "unknown"


def completion_kind_name(kind: int | None) -> str | None:
    """Convert completion item kind integer to string name."""
    if kind is None:
        return None
    try:
        return CompletionItemKind(kind).name.lower().replace("_", " ")
    except ValueError:
        return "unknown"


def severity_name(severity: int | None) -> str:
    """Convert diagnostic severity integer to string name."""
    if severity is None:
        return "unknown"
    try:
        return DiagnosticSeverity(severity).name.lower()
    except ValueError:
        return "unknown"
