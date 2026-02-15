"""CLI for Cangjie LSP MCP server.

Usage:
    cangjie-mcp lsp [OPTIONS]
"""

from __future__ import annotations

import os
import sys
from pathlib import Path
from typing import Annotated

import typer

from cangjie_mcp import __version__
from cangjie_mcp.cli_args import DebugOption, LogFileOption
from cangjie_mcp.config import Settings
from cangjie_mcp.utils import console, setup_logging

lsp_app = typer.Typer(
    name="lsp",
    help="LSP-based code intelligence MCP server",
    invoke_without_command=True,
)


def _lsp_version_callback(value: bool) -> None:
    """Show version and exit."""
    if value:
        console.print(f"cangjie-mcp lsp {__version__}")
        raise typer.Exit()


@lsp_app.callback(invoke_without_command=True)
def lsp_main(
    ctx: typer.Context,
    _version: Annotated[
        bool,
        typer.Option(
            "--version",
            "-v",
            callback=_lsp_version_callback,
            is_eager=True,
            help="Show version and exit",
        ),
    ] = False,
    workspace: Annotated[
        Path | None,
        typer.Option(
            "--workspace",
            "-w",
            help="Workspace root path (default: current directory)",
            envvar="CANGJIE_WORKSPACE",
        ),
    ] = None,
    sdk_path: Annotated[
        Path | None,
        typer.Option(
            "--sdk",
            "-s",
            help="Cangjie SDK path",
            envvar="CANGJIE_HOME",
        ),
    ] = None,
    log_enabled: Annotated[
        bool,
        typer.Option(
            "--log/--no-log",
            help="Enable LSP server logging",
            envvar="CANGJIE_LSP_LOG",
        ),
    ] = False,
    log_path: Annotated[
        Path | None,
        typer.Option(
            "--log-path",
            help="Directory for LSP log files",
            envvar="CANGJIE_LSP_LOG_PATH",
        ),
    ] = None,
    init_timeout: Annotated[
        int,
        typer.Option(
            "--timeout",
            "-t",
            help="LSP initialization timeout in milliseconds",
            envvar="CANGJIE_LSP_TIMEOUT",
        ),
    ] = 45000,
    log_file: LogFileOption = None,
    debug: DebugOption = False,
) -> None:
    """Start the Cangjie LSP MCP server in stdio mode.

    This server provides code intelligence features for Cangjie source files
    through the Model Context Protocol (MCP).

    Requirements:
        - Cangjie SDK installed
        - CANGJIE_HOME environment variable set to SDK path

    Examples:
        # Start with default settings
        cangjie-mcp lsp

        # Specify workspace
        cangjie-mcp lsp --workspace /path/to/project

        # Enable logging
        cangjie-mcp lsp --log --log-path /tmp/cangjie-lsp
    """
    if ctx.invoked_subcommand is not None:
        return

    # Set up logging early
    setup_logging(log_file, debug)

    import asyncio

    from cangjie_mcp.lsp import init, is_available
    from cangjie_mcp.lsp.config import LSPSettings

    # Determine SDK path
    actual_sdk: Path | None = sdk_path
    if actual_sdk is None:
        env_sdk = os.environ.get("CANGJIE_HOME")
        if env_sdk:
            actual_sdk = Path(env_sdk)

    if actual_sdk is None:
        console.print("[red]Error: CANGJIE_HOME not set.[/red]")
        console.print("Please set the CANGJIE_HOME environment variable to your Cangjie SDK path,")
        console.print("or use --sdk to specify the path.")
        raise typer.Exit(1)

    if not actual_sdk.exists():
        console.print(f"[red]Error: SDK path does not exist: {actual_sdk}[/red]")
        raise typer.Exit(1)

    # Check LSP server exists
    exe_name = "LSPServer.exe" if sys.platform == "win32" else "LSPServer"
    lsp_server_path = actual_sdk / "tools" / "bin" / exe_name
    if not lsp_server_path.exists():
        console.print(f"[red]Error: LSP server not found: {lsp_server_path}[/red]")
        console.print("Please ensure your Cangjie SDK installation is complete.")
        raise typer.Exit(1)

    # Determine workspace path
    actual_workspace = workspace if workspace else Path.cwd()
    if not actual_workspace.exists():
        console.print(f"[red]Error: Workspace path does not exist: {actual_workspace}[/red]")
        raise typer.Exit(1)

    # Create settings
    settings = LSPSettings(
        sdk_path=actual_sdk,
        workspace_path=actual_workspace,
        log_enabled=log_enabled,
        log_path=log_path,
        init_timeout=init_timeout,
    )

    # Validate settings
    errors = settings.validate()
    if errors:
        for error in errors:
            console.print(f"[red]Error: {error}[/red]")
        raise typer.Exit(1)

    # Print startup info (console uses stderr, stdout reserved for MCP)
    console.print("[bold]Cangjie LSP MCP Server[/bold]")
    console.print(f"  SDK: {actual_sdk}")
    console.print(f"  Workspace: {actual_workspace}")
    console.print(f"  Logging: {'enabled' if log_enabled else 'disabled'}")
    console.print()

    # Initialize LSP client
    console.print("[blue]Initializing LSP server...[/blue]")

    async def init_lsp() -> bool:
        """Initialize LSP client."""
        success = await init(settings)
        if not success:
            return False
        return is_available()

    try:
        success = asyncio.run(init_lsp())
        if not success:
            console.print("[red]Failed to initialize LSP server[/red]")
            raise typer.Exit(1)

        console.print("[green]LSP server initialized successfully[/green]")

        # Create and run MCP server
        from cangjie_mcp.server.factory import create_mcp_server

        mcp = create_mcp_server(Settings())
        console.print("[blue]Starting MCP server (stdio)...[/blue]")

        # Run MCP server (synchronous, blocks until complete)
        mcp.run(transport="stdio")

    except KeyboardInterrupt:
        console.print("\n[yellow]Interrupted[/yellow]")
    except Exception as e:
        console.print(f"[red]Error: {e}[/red]")
        raise typer.Exit(1) from e
    finally:
        # Cleanup LSP client
        from cangjie_mcp.lsp import shutdown

        asyncio.run(shutdown())


if __name__ == "__main__":
    lsp_app()
