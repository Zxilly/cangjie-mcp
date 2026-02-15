"""Tests for MCP tool schema generation.

Verifies that all registered MCP tools have correctly resolved parameter schemas.
This prevents regressions from `from __future__ import annotations` or other
annotation resolution issues that could cause parameter types to become 'unknown'.
"""

from typing import Any
from unittest.mock import AsyncMock

import pytest
from mcp.server.fastmcp import FastMCP

from cangjie_mcp.server.app import register_docs_tools
from cangjie_mcp.server.lsp_app import register_lsp_tools

# ---- Expected schema definitions for each tool ----

# Maps tool_name -> (input_model_name, expected_fields)
# where expected_fields is a dict of field_name -> expected_type_keyword
DOCS_TOOL_SCHEMAS: dict[str, tuple[str, dict[str, str]]] = {
    "cangjie_search_docs": (
        "SearchDocsInput",
        {
            "query": "string",
            "category": "string",  # anyOf [string, null]
            "top_k": "integer",
            "offset": "integer",
        },
    ),
    "cangjie_get_topic": (
        "GetTopicInput",
        {
            "topic": "string",
            "category": "string",
        },
    ),
    "cangjie_list_topics": (
        "ListTopicsInput",
        {
            "category": "string",
        },
    ),
    "cangjie_get_code_examples": (
        "GetCodeExamplesInput",
        {
            "feature": "string",
            "top_k": "integer",
        },
    ),
    "cangjie_get_tool_usage": (
        "GetToolUsageInput",
        {
            "tool_name": "string",
        },
    ),
    "cangjie_search_stdlib": (
        "SearchStdlibInput",
        {
            "query": "string",
            "package": "string",
            "type_name": "string",
            "include_examples": "boolean",
            "top_k": "integer",
        },
    ),
}

LSP_TOOL_SCHEMAS: dict[str, tuple[str, dict[str, str]]] = {
    "cangjie_lsp_definition": (
        "PositionInput",
        {"file_path": "string", "line": "integer", "character": "integer"},
    ),
    "cangjie_lsp_references": (
        "PositionInput",
        {"file_path": "string", "line": "integer", "character": "integer"},
    ),
    "cangjie_lsp_hover": (
        "PositionInput",
        {"file_path": "string", "line": "integer", "character": "integer"},
    ),
    "cangjie_lsp_symbols": (
        "FileInput",
        {"file_path": "string"},
    ),
    "cangjie_lsp_diagnostics": (
        "FileInput",
        {"file_path": "string"},
    ),
    "cangjie_lsp_completion": (
        "PositionInput",
        {"file_path": "string", "line": "integer", "character": "integer"},
    ),
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


def _get_input_model_schema(tool_params: dict[str, Any], model_name: str) -> dict[str, Any]:
    """Get the input model's property schema from a tool's parameter schema.

    Tool schemas wrap the input model as ``params: $ref -> $defs/ModelName``.
    This helper resolves the reference and returns the model's properties.
    """
    # The model definition should be in $defs
    defs = tool_params.get("$defs", {})
    assert model_name in defs, (
        f"Input model '{model_name}' not found in $defs. "
        f"Available: {list(defs.keys())}. "
        "This may indicate that type annotations were not resolved correctly "
        "(e.g. due to `from __future__ import annotations`)."
    )
    return defs[model_name]


# ---- Fixtures ----


@pytest.fixture(scope="module")
def docs_mcp() -> FastMCP:
    """Create a FastMCP instance with documentation tools registered."""
    mcp = FastMCP("test-docs")
    gate = AsyncMock()
    register_docs_tools(mcp, gate)
    return mcp


@pytest.fixture(scope="module")
def lsp_mcp() -> FastMCP:
    """Create a FastMCP instance with LSP tools registered."""
    mcp = FastMCP("test-lsp")
    register_lsp_tools(mcp)
    return mcp


# ---- Tests ----


class TestDocsToolSchemas:
    """Verify documentation tool schemas are generated correctly."""

    def test_all_docs_tools_registered(self, docs_mcp: FastMCP) -> None:
        """All expected documentation tools are registered."""
        registered = {t.name for t in docs_mcp._tool_manager.list_tools()}
        expected = set(DOCS_TOOL_SCHEMAS.keys())
        assert expected == registered

    @pytest.mark.parametrize("tool_name", list(DOCS_TOOL_SCHEMAS.keys()))
    def test_params_ref_resolves(self, docs_mcp: FastMCP, tool_name: str) -> None:
        """The ``params`` property references a resolved model in $defs."""
        tool = docs_mcp._tool_manager.get_tool(tool_name)
        assert tool is not None
        schema = tool.parameters

        # params should be a $ref, not an opaque/unknown type
        params_prop = schema["properties"]["params"]
        assert "$ref" in params_prop, (
            f"Tool '{tool_name}': 'params' property should be a $ref to the input model, "
            f"got {params_prop}. Type annotations may not be resolved correctly."
        )

        model_name = DOCS_TOOL_SCHEMAS[tool_name][0]
        ref_value = params_prop["$ref"]
        assert ref_value == f"#/$defs/{model_name}"

    @pytest.mark.parametrize("tool_name", list(DOCS_TOOL_SCHEMAS.keys()))
    def test_input_model_fields(self, docs_mcp: FastMCP, tool_name: str) -> None:
        """Input model schema contains all expected fields with correct types."""
        tool = docs_mcp._tool_manager.get_tool(tool_name)
        assert tool is not None

        model_name, expected_fields = DOCS_TOOL_SCHEMAS[tool_name]
        model_schema = _get_input_model_schema(tool.parameters, model_name)
        model_props = model_schema.get("properties", {})

        for field_name, expected_type in expected_fields.items():
            assert field_name in model_props, (
                f"Tool '{tool_name}': field '{field_name}' missing from "
                f"{model_name} schema. Available: {list(model_props.keys())}"
            )
            actual_type = _resolve_schema_type(model_props[field_name])
            assert actual_type == expected_type, (
                f"Tool '{tool_name}': field '{field_name}' has type '{actual_type}', expected '{expected_type}'"
            )

    @pytest.mark.parametrize("tool_name", list(DOCS_TOOL_SCHEMAS.keys()))
    def test_no_unknown_types(self, docs_mcp: FastMCP, tool_name: str) -> None:
        """No field in the schema resolves to 'unknown'."""
        tool = docs_mcp._tool_manager.get_tool(tool_name)
        assert tool is not None

        model_name = DOCS_TOOL_SCHEMAS[tool_name][0]
        model_schema = _get_input_model_schema(tool.parameters, model_name)

        for field_name, prop in model_schema.get("properties", {}).items():
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
        assert expected == registered

    @pytest.mark.parametrize("tool_name", list(LSP_TOOL_SCHEMAS.keys()))
    def test_params_ref_resolves(self, lsp_mcp: FastMCP, tool_name: str) -> None:
        """The ``params`` property references a resolved model in $defs."""
        tool = lsp_mcp._tool_manager.get_tool(tool_name)
        assert tool is not None
        schema = tool.parameters

        params_prop = schema["properties"]["params"]
        assert "$ref" in params_prop, (
            f"Tool '{tool_name}': 'params' property should be a $ref to the input model, "
            f"got {params_prop}. Type annotations may not be resolved correctly."
        )

        model_name = LSP_TOOL_SCHEMAS[tool_name][0]
        ref_value = params_prop["$ref"]
        assert ref_value == f"#/$defs/{model_name}"

    @pytest.mark.parametrize("tool_name", list(LSP_TOOL_SCHEMAS.keys()))
    def test_input_model_fields(self, lsp_mcp: FastMCP, tool_name: str) -> None:
        """Input model schema contains all expected fields with correct types."""
        tool = lsp_mcp._tool_manager.get_tool(tool_name)
        assert tool is not None

        model_name, expected_fields = LSP_TOOL_SCHEMAS[tool_name]
        model_schema = _get_input_model_schema(tool.parameters, model_name)
        model_props = model_schema.get("properties", {})

        for field_name, expected_type in expected_fields.items():
            assert field_name in model_props, (
                f"Tool '{tool_name}': field '{field_name}' missing from "
                f"{model_name} schema. Available: {list(model_props.keys())}"
            )
            actual_type = _resolve_schema_type(model_props[field_name])
            assert actual_type == expected_type, (
                f"Tool '{tool_name}': field '{field_name}' has type '{actual_type}', expected '{expected_type}'"
            )

    @pytest.mark.parametrize("tool_name", list(LSP_TOOL_SCHEMAS.keys()))
    def test_no_unknown_types(self, lsp_mcp: FastMCP, tool_name: str) -> None:
        """No field in the schema resolves to 'unknown'."""
        tool = lsp_mcp._tool_manager.get_tool(tool_name)
        assert tool is not None

        model_name = LSP_TOOL_SCHEMAS[tool_name][0]
        model_schema = _get_input_model_schema(tool.parameters, model_name)

        for field_name, prop in model_schema.get("properties", {}).items():
            resolved = _resolve_schema_type(prop)
            assert resolved != "unknown", (
                f"Tool '{tool_name}': field '{field_name}' resolved to 'unknown'. "
                "This indicates a type annotation resolution failure."
            )
