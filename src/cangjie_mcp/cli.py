"""CLI for Cangjie MCP server.

All CLI options can be configured via environment variables.
Run `cangjie-mcp --help` to see all options and their environment variables.

Commands:
- cangjie-mcp: Combined MCP server with docs + LSP tools (default)
- cangjie-mcp docs: Documentation search only
- cangjie-mcp lsp: LSP code intelligence only

Environment variable naming:
- CANGJIE_* prefix for most options
- OPENAI_* prefix for OpenAI-related options
"""

from pathlib import Path
from typing import Annotated

import typer
from rich.table import Table

from cangjie_mcp import __version__
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
from cangjie_mcp.lsp.cli import lsp_app
from cangjie_mcp.utils import console, create_literal_validator


def _version_callback(value: bool) -> None:
    """Print version and exit."""
    if value:
        console.print(f"cangjie-mcp {__version__}")
        raise typer.Exit()


# Root app - starts combined server by default
app = typer.Typer(
    name="cangjie-mcp",
    help="MCP server for Cangjie programming language (docs + LSP)",
    invoke_without_command=True,
)


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
    # Docs options
    docs_version: Annotated[
        str,
        typer.Option(
            "--docs-version",
            "-V",
            help="Documentation version (git tag)",
            envvar="CANGJIE_DOCS_VERSION",
        ),
    ] = DEFAULT_DOCS_VERSION,
    lang: Annotated[
        str,
        typer.Option(
            "--lang",
            "-l",
            help="Documentation language (zh/en)",
            envvar="CANGJIE_DOCS_LANG",
        ),
    ] = DEFAULT_DOCS_LANG,
    embedding: Annotated[
        str,
        typer.Option(
            "--embedding",
            "-e",
            help="Embedding type (local/openai)",
            envvar="CANGJIE_EMBEDDING_TYPE",
        ),
    ] = DEFAULT_EMBEDDING_TYPE,
    local_model: Annotated[
        str,
        typer.Option(
            "--local-model",
            help="Local HuggingFace embedding model name",
            envvar="CANGJIE_LOCAL_MODEL",
        ),
    ] = DEFAULT_LOCAL_MODEL,
    openai_api_key: Annotated[
        str | None,
        typer.Option(
            "--openai-api-key",
            help="OpenAI API key",
            envvar="OPENAI_API_KEY",
        ),
    ] = None,
    openai_base_url: Annotated[
        str,
        typer.Option(
            "--openai-base-url",
            help="OpenAI API base URL",
            envvar="OPENAI_BASE_URL",
        ),
    ] = DEFAULT_OPENAI_BASE_URL,
    openai_model: Annotated[
        str,
        typer.Option(
            "--openai-model",
            help="OpenAI embedding model",
            envvar="OPENAI_EMBEDDING_MODEL",
        ),
    ] = DEFAULT_OPENAI_MODEL,
    rerank: Annotated[
        str,
        typer.Option(
            "--rerank",
            "-r",
            help="Rerank type (none/local/openai)",
            envvar="CANGJIE_RERANK_TYPE",
        ),
    ] = DEFAULT_RERANK_TYPE,
    rerank_model: Annotated[
        str,
        typer.Option(
            "--rerank-model",
            help="Rerank model name",
            envvar="CANGJIE_RERANK_MODEL",
        ),
    ] = DEFAULT_RERANK_MODEL,
    rerank_top_k: Annotated[
        int,
        typer.Option(
            "--rerank-top-k",
            help="Number of results after reranking",
            envvar="CANGJIE_RERANK_TOP_K",
        ),
    ] = DEFAULT_RERANK_TOP_K,
    rerank_initial_k: Annotated[
        int,
        typer.Option(
            "--rerank-initial-k",
            help="Number of candidates before reranking",
            envvar="CANGJIE_RERANK_INITIAL_K",
        ),
    ] = DEFAULT_RERANK_INITIAL_K,
    chunk_max_size: Annotated[
        int,
        typer.Option(
            "--chunk-size",
            help="Max chunk size in characters",
            envvar="CANGJIE_CHUNK_MAX_SIZE",
        ),
    ] = DEFAULT_CHUNK_MAX_SIZE,
    data_dir: Annotated[
        Path | None,
        typer.Option(
            "--data-dir",
            "-d",
            help="Data directory path",
            envvar="CANGJIE_DATA_DIR",
            show_default="~/.cangjie-mcp",
        ),
    ] = None,
) -> None:
    """Start the combined MCP server with docs and LSP tools.

    This is the default command that provides both documentation search
    and LSP code intelligence features in a single MCP server.
    """
    # If a subcommand is invoked, let it handle execution
    if ctx.invoked_subcommand is not None:
        return

    # Check if LSP is available
    import os

    if not os.environ.get("CANGJIE_HOME"):
        console.print("[yellow]Warning: LSP server not available (CANGJIE_HOME not set)[/yellow]")
        console.print("[yellow]LSP tools will return errors. Set CANGJIE_HOME to enable.[/yellow]")

    # Validate and build settings
    validated_lang = _validate_lang(lang)
    validated_embedding = _validate_embedding_type(embedding)
    validated_rerank = _validate_rerank_type(rerank)

    settings = Settings(
        docs_version=docs_version,
        docs_lang=validated_lang,  # type: ignore[arg-type]
        embedding_type=validated_embedding,  # type: ignore[arg-type]
        local_model=local_model,
        openai_api_key=openai_api_key,
        openai_base_url=openai_base_url,
        openai_model=openai_model,
        rerank_type=validated_rerank,  # type: ignore[arg-type]
        rerank_model=rerank_model,
        rerank_top_k=rerank_top_k,
        rerank_initial_k=rerank_initial_k,
        chunk_max_size=chunk_max_size,
        data_dir=data_dir if data_dir else _default_data_dir(),
    )
    set_settings(settings)

    # Initialize docs index
    initialize_and_index(settings)

    # Create and run combined server
    from cangjie_mcp.server.combined_app import create_combined_mcp_server

    mcp = create_combined_mcp_server(settings)
    console.print("[blue]Starting combined MCP server (docs + LSP)...[/blue]")
    mcp.run(transport="stdio")


# Docs subcommand
docs_app = typer.Typer(
    name="docs",
    help="Documentation search MCP server",
    invoke_without_command=True,
)
app.add_typer(docs_app, name="docs")

# Prebuilt index management (under docs)
prebuilt_app = typer.Typer(help="Prebuilt index management commands")
docs_app.add_typer(prebuilt_app, name="prebuilt")

# LSP subcommand (imported from lsp/cli.py)
app.add_typer(lsp_app, name="lsp")


def _default_data_dir() -> Path:
    """Get the default data directory (~/.cangjie-mcp)."""
    return Path.home() / ".cangjie-mcp"


# Validators created using factory function
_validate_lang = create_literal_validator("language", ("zh", "en"))
_validate_embedding_type = create_literal_validator("embedding type", ("local", "openai"))
_validate_rerank_type = create_literal_validator("rerank type", ("none", "local", "openai"))


def initialize_and_index(settings: Settings) -> None:
    """Initialize repository and build index if needed."""
    from cangjie_mcp.indexer.chunker import create_chunker
    from cangjie_mcp.indexer.embeddings import get_embedding_provider
    from cangjie_mcp.indexer.loader import DocumentLoader
    from cangjie_mcp.indexer.store import create_vector_store
    from cangjie_mcp.prebuilt.manager import PrebuiltManager
    from cangjie_mcp.repo.git_manager import GitManager

    # Check for prebuilt index first
    prebuilt_mgr = PrebuiltManager(settings.data_dir)
    installed = prebuilt_mgr.get_installed_metadata()

    if installed and installed.version == settings.docs_version and installed.lang == settings.docs_lang:
        console.print(
            f"[green]Using prebuilt index (version: {settings.docs_version}, lang: {settings.docs_lang})[/green]"
        )
        return

    # Check existing index
    store = create_vector_store(settings, with_rerank=False)

    if store.is_indexed() and store.version_matches(settings.docs_version, settings.docs_lang):
        console.print(
            f"[green]Index already exists (version: {settings.docs_version}, lang: {settings.docs_lang})[/green]"
        )
        return

    # Need to build index - ensure repo is ready
    console.print("[blue]Building new index...[/blue]")

    git_mgr = GitManager(settings.docs_repo_dir)
    git_mgr.ensure_cloned()

    # Checkout correct version
    current_version = git_mgr.get_current_version()
    if current_version != settings.docs_version:
        git_mgr.checkout(settings.docs_version)

    # Load documents
    loader = DocumentLoader(settings.docs_source_dir)
    documents = loader.load_all_documents()

    if not documents:
        console.print("[red]No documents found![/red]")
        raise typer.Exit(1)

    # Chunk documents
    embedding_provider = get_embedding_provider(settings)
    chunker = create_chunker(embedding_provider, max_chunk_size=settings.chunk_max_size)
    nodes = chunker.chunk_documents(documents, use_semantic=True)

    # Index
    store.index_nodes(nodes)
    store.save_metadata(
        version=settings.docs_version,
        lang=settings.docs_lang,
        embedding_model=embedding_provider.get_model_name(),
    )

    console.print("[green]Index built successfully![/green]")


@docs_app.callback(invoke_without_command=True)
def docs_main(
    ctx: typer.Context,
    docs_version: Annotated[
        str,
        typer.Option(
            "--docs-version",
            "-V",
            help="Documentation version (git tag)",
            envvar="CANGJIE_DOCS_VERSION",
            show_default=True,
        ),
    ] = DEFAULT_DOCS_VERSION,
    lang: Annotated[
        str,
        typer.Option(
            "--lang",
            "-l",
            help="Documentation language (zh/en)",
            envvar="CANGJIE_DOCS_LANG",
            show_default=True,
        ),
    ] = DEFAULT_DOCS_LANG,
    embedding: Annotated[
        str,
        typer.Option(
            "--embedding",
            "-e",
            help="Embedding type (local/openai)",
            envvar="CANGJIE_EMBEDDING_TYPE",
            show_default=True,
        ),
    ] = DEFAULT_EMBEDDING_TYPE,
    local_model: Annotated[
        str,
        typer.Option(
            "--local-model",
            help="Local HuggingFace embedding model name",
            envvar="CANGJIE_LOCAL_MODEL",
            show_default=True,
        ),
    ] = DEFAULT_LOCAL_MODEL,
    openai_api_key: Annotated[
        str | None,
        typer.Option(
            "--openai-api-key",
            help="OpenAI API key",
            envvar="OPENAI_API_KEY",
        ),
    ] = None,
    openai_base_url: Annotated[
        str,
        typer.Option(
            "--openai-base-url",
            help="OpenAI API base URL",
            envvar="OPENAI_BASE_URL",
            show_default=True,
        ),
    ] = DEFAULT_OPENAI_BASE_URL,
    openai_model: Annotated[
        str,
        typer.Option(
            "--openai-model",
            help="OpenAI embedding model",
            envvar="OPENAI_EMBEDDING_MODEL",
            show_default=True,
        ),
    ] = DEFAULT_OPENAI_MODEL,
    rerank: Annotated[
        str,
        typer.Option(
            "--rerank",
            "-r",
            help="Rerank type (none/local/openai)",
            envvar="CANGJIE_RERANK_TYPE",
            show_default=True,
        ),
    ] = DEFAULT_RERANK_TYPE,
    rerank_model: Annotated[
        str,
        typer.Option(
            "--rerank-model",
            help="Rerank model name",
            envvar="CANGJIE_RERANK_MODEL",
            show_default=True,
        ),
    ] = DEFAULT_RERANK_MODEL,
    rerank_top_k: Annotated[
        int,
        typer.Option(
            "--rerank-top-k",
            help="Number of results after reranking",
            envvar="CANGJIE_RERANK_TOP_K",
            show_default=True,
        ),
    ] = DEFAULT_RERANK_TOP_K,
    rerank_initial_k: Annotated[
        int,
        typer.Option(
            "--rerank-initial-k",
            help="Number of candidates before reranking",
            envvar="CANGJIE_RERANK_INITIAL_K",
            show_default=True,
        ),
    ] = DEFAULT_RERANK_INITIAL_K,
    chunk_size: Annotated[
        int,
        typer.Option(
            "--chunk-size",
            help="Max chunk size in characters",
            envvar="CANGJIE_CHUNK_MAX_SIZE",
            show_default=True,
        ),
    ] = DEFAULT_CHUNK_MAX_SIZE,
    data_dir: Annotated[
        Path | None,
        typer.Option(
            "--data-dir",
            "-d",
            help="Data directory path",
            envvar="CANGJIE_DATA_DIR",
            show_default="~/.cangjie-mcp",
        ),
    ] = None,
) -> None:
    """Start the documentation MCP server in stdio mode.

    Starts the MCP server using stdio transport for MCP client integration.
    """
    # If a subcommand is invoked, let it handle execution
    if ctx.invoked_subcommand is not None:
        return

    # Validate literal types
    validated_lang = _validate_lang(lang)
    validated_embedding = _validate_embedding_type(embedding)
    validated_rerank = _validate_rerank_type(rerank)

    settings = Settings(
        docs_version=docs_version,
        docs_lang=validated_lang,  # type: ignore[arg-type]
        embedding_type=validated_embedding,  # type: ignore[arg-type]
        local_model=local_model,
        openai_api_key=openai_api_key,
        openai_base_url=openai_base_url,
        openai_model=openai_model,
        rerank_type=validated_rerank,  # type: ignore[arg-type]
        rerank_model=rerank_model,
        rerank_top_k=rerank_top_k,
        rerank_initial_k=rerank_initial_k,
        chunk_max_size=chunk_size,
        data_dir=data_dir if data_dir else Settings().data_dir,
    )
    set_settings(settings)

    console.print("[bold]Cangjie MCP Server (stdio)[/bold]")
    console.print(f"  Version: {settings.docs_version}")
    console.print(f"  Language: {settings.docs_lang}")
    console.print(f"  Embedding: {settings.embedding_type}")
    console.print(f"  Rerank: {settings.rerank_type}")
    if settings.rerank_type != "none":
        console.print(f"  Rerank Model: {settings.rerank_model}")
    console.print()

    # Initialize and index
    initialize_and_index(settings)

    # Start server in stdio mode
    from cangjie_mcp.server.app import create_mcp_server

    mcp = create_mcp_server(settings)
    console.print("[blue]Starting MCP server (stdio)...[/blue]")
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
        docs_lang=_validate_lang(lang) if lang else defaults.docs_lang,  # type: ignore[arg-type]
        embedding_type=_validate_embedding_type(embedding) if embedding else defaults.embedding_type,  # type: ignore[arg-type]
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

    # Show currently installed index (for stdio mode)
    installed = mgr.get_installed_metadata()
    if installed:
        console.print()
        console.print("[bold]Currently Installed (stdio mode):[/bold]")
        console.print(f"  Version: {installed.version}")
        console.print(f"  Language: {installed.lang}")
        console.print(f"  Embedding: {installed.embedding_model}")


if __name__ == "__main__":
    app()
