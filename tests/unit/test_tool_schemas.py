"""Tests for MCP tool schema generation.

Verifies that all registered MCP tools have correctly resolved parameter schemas.
This prevents regressions from `from __future__ import annotations` or other
annotation resolution issues that could cause parameter types to become 'unknown'.
"""

from typing import Any

import pytest
from mcp.server.fastmcp import FastMCP

from cangjie_mcp.server.tools import mcp

# ---- Expected schema definitions for each tool ----

# Maps tool_name -> dict of field_name -> expected_type_keyword
DOCS_TOOL_SCHEMAS: dict[str, dict[str, str]] = {
    "cangjie_search_docs": {
        "query": "string",
        "category": "string",
        "top_k": "integer",
        "offset": "integer",
    },
    "cangjie_get_topic": {
        "topic": "string",
        "category": "string",
    },
    "cangjie_list_topics": {
        "category": "string",
    },
    "cangjie_get_code_examples": {
        "feature": "string",
        "top_k": "integer",
    },
    "cangjie_get_tool_usage": {
        "tool_name": "string",
    },
    "cangjie_search_stdlib": {
        "query": "string",
        "package": "string",
        "type_name": "string",
        "include_examples": "boolean",
        "top_k": "integer",
    },
}

LSP_TOOL_SCHEMAS: dict[str, dict[str, str]] = {
    "cangjie_lsp_definition": {
        "file_path": "string",
        "line": "integer",
        "character": "integer",
    },
    "cangjie_lsp_references": {
        "file_path": "string",
        "line": "integer",
        "character": "integer",
    },
    "cangjie_lsp_hover": {
        "file_path": "string",
        "line": "integer",
        "character": "integer",
    },
    "cangjie_lsp_symbols": {
        "file_path": "string",
    },
    "cangjie_lsp_diagnostics": {
        "file_path": "string",
    },
    "cangjie_lsp_completion": {
        "file_path": "string",
        "line": "integer",
        "character": "integer",
    },
}


def _resolve_schema_type(prop: dict[str, Any]) -> str:
    """Extract the type keyword from a JSON schema property.

    Handles both direct types and anyOf unions (e.g. ``str | None``).
    """
    if "type" in prop:
        return prop["type"]
    if "anyOf" in prop:
        # For unions like str | None, return the non-null type
        for variant in prop["anyOf"]:
            if variant.get("type") != "null":
                return variant["type"]
    return "unknown"


# ---- Fixtures ----


@pytest.fixture(scope="module")
def docs_mcp() -> FastMCP:
    """Return the module-level mcp instance (docs tools registered at import)."""
    return mcp


@pytest.fixture(scope="module")
def lsp_mcp() -> FastMCP:
    """Return the module-level mcp instance with LSP tools registered."""
    import cangjie_mcp.lsp.tools  # noqa: F401  # pyright: ignore[reportUnusedImport]

    return mcp


# ---- Tests ----


class TestDocsToolSchemas:
    """Verify documentation tool schemas are generated correctly."""

    def test_all_docs_tools_registered(self, docs_mcp: FastMCP) -> None:
        """All expected documentation tools are registered."""
        registered = {t.name for t in docs_mcp._tool_manager.list_tools()}
        expected = set(DOCS_TOOL_SCHEMAS.keys())
        assert expected.issubset(registered)

    @pytest.mark.parametrize("tool_name", list(DOCS_TOOL_SCHEMAS.keys()))
    def test_fields_are_top_level(self, docs_mcp: FastMCP, tool_name: str) -> None:
        """Tool parameters are flat top-level properties (not wrapped in a params object)."""
        tool = docs_mcp._tool_manager.get_tool(tool_name)
        assert tool is not None
        schema = tool.parameters
        props = schema.get("properties", {})

        assert "params" not in props, f"Tool '{tool_name}': should use flat parameters, not a 'params' wrapper"

        expected_fields = DOCS_TOOL_SCHEMAS[tool_name]
        for field_name in expected_fields:
            assert field_name in props, (
                f"Tool '{tool_name}': field '{field_name}' missing from schema. Available: {list(props.keys())}"
            )

    @pytest.mark.parametrize("tool_name", list(DOCS_TOOL_SCHEMAS.keys()))
    def test_field_types(self, docs_mcp: FastMCP, tool_name: str) -> None:
        """All fields have correct types."""
        tool = docs_mcp._tool_manager.get_tool(tool_name)
        assert tool is not None
        props = tool.parameters.get("properties", {})

        expected_fields = DOCS_TOOL_SCHEMAS[tool_name]
        for field_name, expected_type in expected_fields.items():
            assert field_name in props
            actual_type = _resolve_schema_type(props[field_name])
            assert actual_type == expected_type, (
                f"Tool '{tool_name}': field '{field_name}' has type '{actual_type}', expected '{expected_type}'"
            )

    @pytest.mark.parametrize("tool_name", list(DOCS_TOOL_SCHEMAS.keys()))
    def test_no_unknown_types(self, docs_mcp: FastMCP, tool_name: str) -> None:
        """No field in the schema resolves to 'unknown'."""
        tool = docs_mcp._tool_manager.get_tool(tool_name)
        assert tool is not None
        props = tool.parameters.get("properties", {})

        for field_name, prop in props.items():
            resolved = _resolve_schema_type(prop)
            assert resolved != "unknown", (
                f"Tool '{tool_name}': field '{field_name}' resolved to 'unknown'. "
                "This indicates a type annotation resolution failure."
            )


class TestLspToolSchemas:
    """Verify LSP tool schemas are generated correctly."""

    def test_all_lsp_tools_registered(self, lsp_mcp: FastMCP) -> None:
        """All expected LSP tools are registered."""
        registered = {t.name for t in lsp_mcp._tool_manager.list_tools()}
        expected = set(LSP_TOOL_SCHEMAS.keys())
        assert expected.issubset(registered)

    @pytest.mark.parametrize("tool_name", list(LSP_TOOL_SCHEMAS.keys()))
    def test_fields_are_top_level(self, lsp_mcp: FastMCP, tool_name: str) -> None:
        """Tool parameters are flat top-level properties (not wrapped in a params object)."""
        tool = lsp_mcp._tool_manager.get_tool(tool_name)
        assert tool is not None
        schema = tool.parameters
        props = schema.get("properties", {})

        assert "params" not in props, f"Tool '{tool_name}': should use flat parameters, not a 'params' wrapper"

        expected_fields = LSP_TOOL_SCHEMAS[tool_name]
        for field_name in expected_fields:
            assert field_name in props, (
                f"Tool '{tool_name}': field '{field_name}' missing from schema. Available: {list(props.keys())}"
            )

    @pytest.mark.parametrize("tool_name", list(LSP_TOOL_SCHEMAS.keys()))
    def test_field_types(self, lsp_mcp: FastMCP, tool_name: str) -> None:
        """All fields have correct types."""
        tool = lsp_mcp._tool_manager.get_tool(tool_name)
        assert tool is not None
        props = tool.parameters.get("properties", {})

        expected_fields = LSP_TOOL_SCHEMAS[tool_name]
        for field_name, expected_type in expected_fields.items():
            assert field_name in props
            actual_type = _resolve_schema_type(props[field_name])
            assert actual_type == expected_type, (
                f"Tool '{tool_name}': field '{field_name}' has type '{actual_type}', expected '{expected_type}'"
            )

    @pytest.mark.parametrize("tool_name", list(LSP_TOOL_SCHEMAS.keys()))
    def test_no_unknown_types(self, lsp_mcp: FastMCP, tool_name: str) -> None:
        """No field in the schema resolves to 'unknown'."""
        tool = lsp_mcp._tool_manager.get_tool(tool_name)
        assert tool is not None
        props = tool.parameters.get("properties", {})

        for field_name, prop in props.items():
            resolved = _resolve_schema_type(prop)
            assert resolved != "unknown", (
                f"Tool '{tool_name}': field '{field_name}' resolved to 'unknown'. "
                "This indicates a type annotation resolution failure."
            )
