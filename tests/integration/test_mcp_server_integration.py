"""Integration tests for MCP server creation.

These tests verify the MCP server can be created and has expected tools.
"""

import pytest

from cangjie_mcp.config import Settings
from cangjie_mcp.server.app import create_mcp_server


class TestMCPServerCreation:
    """Integration tests for MCP server creation."""

    def test_create_mcp_server(self, local_settings: Settings) -> None:
        """Test that MCP server can be created successfully."""
        mcp = create_mcp_server(local_settings)

        assert mcp is not None
        assert mcp.name == "cangjie-docs"

    @pytest.mark.asyncio
    async def test_mcp_server_has_tools(self, local_settings: Settings) -> None:
        """Test that MCP server has expected tools registered."""
        mcp = create_mcp_server(local_settings)

        tools_list = await mcp.list_tools()
        tool_names = [tool.name for tool in tools_list]
        expected_tools = [
            "search_docs",
            "get_topic",
            "list_topics",
            "get_code_examples",
            "get_tool_usage",
        ]

        for expected in expected_tools:
            assert expected in tool_names, f"Tool '{expected}' not found in MCP server"

    def test_mcp_server_name(self, local_settings: Settings) -> None:
        """Test MCP server has correct name."""
        mcp = create_mcp_server(local_settings)
        assert mcp.name == "cangjie-docs"

    @pytest.mark.asyncio
    async def test_mcp_server_tool_count(self, local_settings: Settings) -> None:
        """Test MCP server has expected number of tools."""
        mcp = create_mcp_server(local_settings)

        tools_list = await mcp.list_tools()
        # Should have at least 5 tools
        assert len(tools_list) >= 5

    @pytest.mark.asyncio
    async def test_mcp_server_tool_descriptions(self, local_settings: Settings) -> None:
        """Test that all MCP tools have descriptions."""
        mcp = create_mcp_server(local_settings)

        tools_list = await mcp.list_tools()
        for tool in tools_list:
            assert tool.description, f"Tool '{tool.name}' has no description"
            assert len(tool.description) > 10, f"Tool '{tool.name}' has too short description"
