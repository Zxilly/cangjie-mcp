"""CLI for Cangjie MCP server.

All CLI options can be configured via environment variables.
Run `cangjie-mcp --help` to see all options and their environment variables.

Environment variable naming:
- CANGJIE_* prefix for most options
- OPENAI_* prefix for OpenAI-related options
"""

from pathlib import Path
from typing import Annotated

import typer
from rich.table import Table

from cangjie_mcp import __version__
from cangjie_mcp.cli_args import (
    ChunkSizeOption,
    DataDirOption,
    DebugOption,
    DocsVersionOption,
    EmbeddingOption,
    LangOption,
    LocalModelOption,
    LogFileOption,
    OpenAIApiKeyOption,
    OpenAIBaseUrlOption,
    OpenAIModelOption,
    PrebuiltUrlOption,
    RerankInitialKOption,
    RerankModelOption,
    RerankOption,
    RerankTopKOption,
    validate_embedding_type,
    validate_lang,
    validate_rerank_type,
)
from cangjie_mcp.config import Settings, set_settings
from cangjie_mcp.defaults import (
    DEFAULT_CHUNK_MAX_SIZE,
    DEFAULT_DOCS_LANG,
    DEFAULT_DOCS_VERSION,
    DEFAULT_EMBEDDING_TYPE,
    DEFAULT_LOCAL_MODEL,
    DEFAULT_OPENAI_BASE_URL,
    DEFAULT_OPENAI_MODEL,
    DEFAULT_RERANK_INITIAL_K,
    DEFAULT_RERANK_MODEL,
    DEFAULT_RERANK_TOP_K,
    DEFAULT_RERANK_TYPE,
)
from cangjie_mcp.factory import create_settings_from_args
from cangjie_mcp.indexer.initializer import initialize_and_index
from cangjie_mcp.utils import console, setup_logging


def _version_callback(value: bool) -> None:
    """Print version and exit."""
    if value:
        console.print(f"cangjie-mcp {__version__}")
        raise typer.Exit()


def _default_data_dir() -> Path:
    """Get the default data directory (~/.cangjie-mcp)."""
    return Path.home() / ".cangjie-mcp"


# Root app
app = typer.Typer(
    name="cangjie-mcp",
    help="MCP server for Cangjie programming language",
    invoke_without_command=True,
)

# Prebuilt index management
prebuilt_app = typer.Typer(help="Prebuilt index management commands")
app.add_typer(prebuilt_app, name="prebuilt")


@app.callback(invoke_without_command=True)
def main(
    ctx: typer.Context,
    _version: Annotated[
        bool,
        typer.Option(
            "--version",
            "-v",
            help="Show version and exit",
            callback=_version_callback,
            is_eager=True,
        ),
    ] = False,
    # Docs options - using shared type aliases
    docs_version: DocsVersionOption = DEFAULT_DOCS_VERSION,
    lang: LangOption = DEFAULT_DOCS_LANG,
    embedding: EmbeddingOption = DEFAULT_EMBEDDING_TYPE,
    local_model: LocalModelOption = DEFAULT_LOCAL_MODEL,
    openai_api_key: OpenAIApiKeyOption = None,
    openai_base_url: OpenAIBaseUrlOption = DEFAULT_OPENAI_BASE_URL,
    openai_model: OpenAIModelOption = DEFAULT_OPENAI_MODEL,
    rerank: RerankOption = DEFAULT_RERANK_TYPE,
    rerank_model: RerankModelOption = DEFAULT_RERANK_MODEL,
    rerank_top_k: RerankTopKOption = DEFAULT_RERANK_TOP_K,
    rerank_initial_k: RerankInitialKOption = DEFAULT_RERANK_INITIAL_K,
    chunk_max_size: ChunkSizeOption = DEFAULT_CHUNK_MAX_SIZE,
    data_dir: DataDirOption = None,
    prebuilt_url: PrebuiltUrlOption = None,
    log_file: LogFileOption = None,
    debug: DebugOption = False,
) -> None:
    """Start the MCP server."""
    # If a subcommand is invoked, let it handle execution
    if ctx.invoked_subcommand is not None:
        return

    # Set up logging early
    setup_logging(log_file, debug)

    # Check if LSP is available
    import os

    lsp_enabled = bool(os.environ.get("CANGJIE_HOME"))
    if not lsp_enabled:
        console.print("[yellow]CANGJIE_HOME not set — LSP tools will not be registered.[/yellow]")

    # Validate and build settings
    settings = create_settings_from_args(
        docs_version=docs_version,
        docs_lang=validate_lang(lang),  # type: ignore[arg-type]
        embedding_type=validate_embedding_type(embedding),  # type: ignore[arg-type]
        local_model=local_model,
        openai_api_key=openai_api_key,
        openai_base_url=openai_base_url,
        openai_model=openai_model,
        rerank_type=validate_rerank_type(rerank),  # type: ignore[arg-type]
        rerank_model=rerank_model,
        rerank_top_k=rerank_top_k,
        rerank_initial_k=rerank_initial_k,
        chunk_max_size=chunk_max_size,
        data_dir=data_dir,
        prebuilt_url=prebuilt_url,
    )
    set_settings(settings)

    # Initialize docs index
    initialize_and_index(settings)

    # Create and run server
    from cangjie_mcp.server.factory import create_mcp_server

    mcp = create_mcp_server(settings, lsp_enabled=lsp_enabled)
    console.print("[blue]Starting MCP server...[/blue]")
    mcp.run(transport="stdio")


@prebuilt_app.command("download")
def prebuilt_download(
    url: Annotated[
        str | None,
        typer.Option(
            "--url",
            "-u",
            help="URL to download from",
            envvar="CANGJIE_PREBUILT_URL",
        ),
    ] = None,
    version: Annotated[
        str | None,
        typer.Option(
            "--version",
            "-v",
            help="Version to download",
            envvar="CANGJIE_DOCS_VERSION",
        ),
    ] = None,
    lang: Annotated[
        str | None,
        typer.Option(
            "--lang",
            "-l",
            help="Language to download",
            envvar="CANGJIE_DOCS_LANG",
        ),
    ] = None,
    data_dir: Annotated[
        Path | None,
        typer.Option(
            "--data-dir",
            "-d",
            help="Data directory path",
            envvar="CANGJIE_DATA_DIR",
        ),
    ] = None,
) -> None:
    """Download a prebuilt index."""
    from cangjie_mcp.prebuilt.manager import PrebuiltManager

    if not url:
        console.print("[red]No URL provided. Set CANGJIE_PREBUILT_URL or use --url[/red]")
        raise typer.Exit(1)

    actual_version = version or DEFAULT_DOCS_VERSION
    actual_lang = lang or DEFAULT_DOCS_LANG
    actual_data_dir = data_dir or _default_data_dir()

    mgr = PrebuiltManager(actual_data_dir)
    try:
        archive_path = mgr.download(url, actual_version, actual_lang)
        mgr.install(archive_path)
    except Exception as e:
        console.print(f"[red]Failed to download: {e}[/red]")
        raise typer.Exit(1) from None


@prebuilt_app.command("build")
def prebuilt_build(
    version: Annotated[
        str | None,
        typer.Option(
            "--version",
            "-v",
            help="Documentation version (git tag)",
            envvar="CANGJIE_DOCS_VERSION",
        ),
    ] = None,
    lang: Annotated[
        str | None,
        typer.Option(
            "--lang",
            "-l",
            help="Documentation language (zh/en)",
            envvar="CANGJIE_DOCS_LANG",
        ),
    ] = None,
    embedding: Annotated[
        str | None,
        typer.Option(
            "--embedding",
            "-e",
            help="Embedding type (local/openai)",
            envvar="CANGJIE_EMBEDDING_TYPE",
        ),
    ] = None,
    local_model: Annotated[
        str | None,
        typer.Option(
            "--local-model",
            help="Local embedding model name",
            envvar="CANGJIE_LOCAL_MODEL",
        ),
    ] = None,
    openai_api_key: Annotated[
        str | None,
        typer.Option(
            "--openai-api-key",
            help="OpenAI API key",
            envvar="OPENAI_API_KEY",
        ),
    ] = None,
    openai_base_url: Annotated[
        str | None,
        typer.Option(
            "--openai-base-url",
            help="OpenAI API base URL",
            envvar="OPENAI_BASE_URL",
        ),
    ] = None,
    openai_model: Annotated[
        str | None,
        typer.Option(
            "--openai-model",
            help="OpenAI embedding model",
            envvar="OPENAI_EMBEDDING_MODEL",
        ),
    ] = None,
    chunk_size: Annotated[
        int | None,
        typer.Option(
            "--chunk-size",
            "-c",
            help="Max chunk size in characters",
            envvar="CANGJIE_CHUNK_MAX_SIZE",
        ),
    ] = None,
    data_dir: Annotated[
        Path | None,
        typer.Option(
            "--data-dir",
            "-d",
            help="Data directory",
            envvar="CANGJIE_DATA_DIR",
        ),
    ] = None,
    output: Annotated[
        Path | None,
        typer.Option("--output", "-o", help="Output directory or file path"),
    ] = None,
) -> None:
    """Build a prebuilt index archive.

    Automatically clones documentation repository and builds the vector index
    before creating the archive.
    """
    from cangjie_mcp.indexer.chunker import create_chunker
    from cangjie_mcp.indexer.embeddings import create_embedding_provider
    from cangjie_mcp.indexer.loader import DocumentLoader
    from cangjie_mcp.indexer.store import VectorStore
    from cangjie_mcp.prebuilt.manager import PrebuiltManager
    from cangjie_mcp.repo.git_manager import GitManager

    # Build settings with optional overrides
    defaults = Settings()
    settings = Settings(
        docs_version=version if version else defaults.docs_version,
        docs_lang=validate_lang(lang) if lang else defaults.docs_lang,  # type: ignore[arg-type]
        embedding_type=validate_embedding_type(embedding) if embedding else defaults.embedding_type,  # type: ignore[arg-type]
        local_model=local_model if local_model else defaults.local_model,
        openai_api_key=openai_api_key,
        openai_base_url=openai_base_url if openai_base_url else defaults.openai_base_url,
        openai_model=openai_model if openai_model else defaults.openai_model,
        chunk_max_size=chunk_size if chunk_size else defaults.chunk_max_size,
        data_dir=data_dir if data_dir else defaults.data_dir,
    )
    set_settings(settings)

    console.print("[bold]Building Prebuilt Index Archive[/bold]")
    console.print(f"  Version: {settings.docs_version}")
    console.print(f"  Language: {settings.docs_lang}")
    console.print(f"  Embedding: {settings.embedding_type}")
    console.print(f"  Chunk size: {settings.chunk_max_size}")
    console.print(f"  Data dir: {settings.data_dir}")
    console.print()

    # Step 1: Ensure repo is ready
    console.print("[blue]Ensuring documentation repository...[/blue]")
    git_mgr = GitManager(settings.docs_repo_dir)
    git_mgr.ensure_cloned()

    current_version = git_mgr.get_current_version()
    if current_version != settings.docs_version:
        console.print(f"[blue]Checking out version {settings.docs_version}...[/blue]")
        git_mgr.checkout(settings.docs_version)

    # Step 2: Load documents
    console.print("[blue]Loading documents...[/blue]")
    loader = DocumentLoader(settings.docs_source_dir)
    documents = loader.load_all_documents()

    if not documents:
        console.print("[red]No documents found![/red]")
        raise typer.Exit(1)

    console.print(f"  Loaded {len(documents)} documents")

    # Step 3: Chunk documents
    console.print("[blue]Chunking documents...[/blue]")
    embedding_provider = create_embedding_provider(settings)
    chunker = create_chunker(embedding_provider, max_chunk_size=settings.chunk_max_size)
    nodes = chunker.chunk_documents(documents, use_semantic=True)
    console.print(f"  Created {len(nodes)} chunks")

    # Step 4: Build index
    console.print("[blue]Building index...[/blue]")
    store = VectorStore(
        db_path=settings.chroma_db_dir,
        embedding_provider=embedding_provider,
    )
    store.index_nodes(nodes)
    store.save_metadata(
        version=settings.docs_version,
        lang=settings.docs_lang,
        embedding_model=embedding_provider.get_model_name(),
    )
    console.print("[green]Index built successfully![/green]")

    # Step 5: Create archive
    console.print("[blue]Creating archive...[/blue]")
    mgr = PrebuiltManager(settings.index_dir)

    try:
        archive_path = mgr.build(
            version=settings.docs_version,
            lang=settings.docs_lang,
            embedding_model=embedding_provider.get_model_name(),
            docs_source_dir=settings.docs_source_dir,
            output_path=output,
        )
        console.print(f"[green]Archive built: {archive_path}[/green]")
    except Exception as e:
        console.print(f"[red]Failed to build archive: {e}[/red]")
        raise typer.Exit(1) from None


@prebuilt_app.command("list")
def prebuilt_list(
    data_dir: Annotated[
        Path | None,
        typer.Option(
            "--data-dir",
            "-d",
            help="Data directory path",
            envvar="CANGJIE_DATA_DIR",
        ),
    ] = None,
) -> None:
    """List available prebuilt indexes."""
    from cangjie_mcp.prebuilt.manager import PrebuiltManager

    actual_data_dir = data_dir or _default_data_dir()
    mgr = PrebuiltManager(actual_data_dir)

    # List local archives
    local = mgr.list_local()

    if not local:
        console.print("[yellow]No local prebuilt indexes found.[/yellow]")
    else:
        table = Table(title="Local Prebuilt Indexes")
        table.add_column("Version")
        table.add_column("Language")
        table.add_column("Embedding")
        table.add_column("Path")

        for item in local:
            table.add_row(
                item.version,
                item.lang,
                item.embedding_model,
                item.path,
            )

        console.print(table)

    # Show currently installed index
    installed = mgr.get_installed_metadata()
    if installed:
        console.print()
        console.print("[bold]Currently Installed:[/bold]")
        console.print(f"  Version: {installed.version}")
        console.print(f"  Language: {installed.lang}")
        console.print(f"  Embedding: {installed.embedding_model}")


if __name__ == "__main__":
    app()
