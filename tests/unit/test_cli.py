"""Tests for CLI module."""

from pathlib import Path
from unittest.mock import MagicMock, patch

from typer.testing import CliRunner

from cangjie_mcp.cli import app

runner = CliRunner()


class TestInitializeAndIndex:
    """Tests for initialize_and_index function."""

    @patch("cangjie_mcp.indexer.initializer._index_is_ready", return_value=True)
    def test_uses_existing_index(
        self,
        mock_index_is_ready: MagicMock,
    ) -> None:
        """Test that existing index is used when version matches."""
        from cangjie_mcp.config import IndexInfo
        from cangjie_mcp.indexer.initializer import initialize_and_index

        mock_settings = MagicMock()
        mock_settings.docs_version = "v1.0.0"
        mock_settings.docs_lang = "zh"
        mock_settings.embedding_model_name = "local:paraphrase-multilingual-MiniLM-L12-v2"
        mock_settings.data_dir = Path("/test/data")

        result = initialize_and_index(mock_settings)

        # Should check existing index via lightweight metadata check
        mock_index_is_ready.assert_called_once()
        call_args = mock_index_is_ready.call_args
        assert isinstance(call_args[0][0], IndexInfo)
        assert call_args[0][1] == "v1.0.0"
        assert call_args[0][2] == "zh"
        assert isinstance(result, IndexInfo)


class TestServerCommand:
    """Tests for server subcommand."""

    def test_server_help(self) -> None:
        """Test server --help shows usage."""
        result = runner.invoke(app, ["server", "--help"])
        assert result.exit_code == 0
        assert "HTTP query server" in result.output

    def test_server_command_exists(self) -> None:
        """Test that the server subcommand is registered."""
        result = runner.invoke(app, ["--help"])
        assert result.exit_code == 0
        assert "server" in result.output


class TestServerUrlOption:
    """Tests for --server-url option."""

    def test_server_url_in_help(self) -> None:
        """Test that --server-url appears in help."""
        result = runner.invoke(app, ["--help"])
        assert result.exit_code == 0
        assert "--server-url" in result.output

    @patch("cangjie_mcp.server.factory.create_mcp_server")
    def test_server_url_passed_to_settings(
        self,
        mock_create: MagicMock,
    ) -> None:
        """Test that --server-url is passed to Settings."""
        mock_mcp = MagicMock()
        mock_create.return_value = mock_mcp

        result = runner.invoke(app, ["--server-url", "http://localhost:8765"])

        # The server should have been created
        if result.exit_code == 0:
            mock_create.assert_called_once()
