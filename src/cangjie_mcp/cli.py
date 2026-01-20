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
from rich.console import Console
from rich.table import Table

from cangjie_mcp.config import Settings, set_settings

app = typer.Typer(
    name="cangjie-mcp",
    help="MCP server for Cangjie programming language documentation",
    invoke_without_command=True,
)

prebuilt_app = typer.Typer(help="Prebuilt index management commands")
app.add_typer(prebuilt_app, name="prebuilt")

console = Console()

# Default values (centralized)
DEFAULT_DOCS_VERSION = "latest"
DEFAULT_DOCS_LANG = "zh"
DEFAULT_EMBEDDING_TYPE = "local"
DEFAULT_LOCAL_MODEL = "paraphrase-multilingual-MiniLM-L12-v2"
DEFAULT_OPENAI_BASE_URL = "https://api.openai.com/v1"
DEFAULT_OPENAI_MODEL = "text-embedding-3-small"
DEFAULT_RERANK_TYPE = "none"
DEFAULT_RERANK_MODEL = "BAAI/bge-reranker-v2-m3"
DEFAULT_RERANK_TOP_K = 5
DEFAULT_RERANK_INITIAL_K = 20
DEFAULT_CHUNK_MAX_SIZE = 6000
DEFAULT_HTTP_HOST = "127.0.0.1"
DEFAULT_HTTP_PORT = 8000


def _get_default_data_dir() -> Path:
    """Get the default data directory (~/.cangjie-mcp)."""
    return Path.home() / ".cangjie-mcp"


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


def _create_settings(
    *,
    version: str | None = None,
    lang: str | None = None,
    embedding: str | None = None,
    local_model: str | None = None,
    openai_api_key: str | None = None,
    openai_base_url: str | None = None,
    openai_model: str | None = None,
    rerank: str | None = None,
    rerank_model: str | None = None,
    rerank_top_k: int | None = None,
    rerank_initial_k: int | None = None,
    chunk_size: int | None = None,
    data_dir: Path | None = None,
    prebuilt_url: str | None = None,
    http_host: str | None = None,
    http_port: int | None = None,
    indexes: str | None = None,
) -> Settings:
    """Create Settings instance from CLI arguments."""
    settings = Settings(
        docs_version=version or DEFAULT_DOCS_VERSION,
        docs_lang=lang or DEFAULT_DOCS_LANG,  # type: ignore[arg-type]
        embedding_type=embedding or DEFAULT_EMBEDDING_TYPE,  # type: ignore[arg-type]
        local_model=local_model or DEFAULT_LOCAL_MODEL,
        openai_api_key=openai_api_key,
        openai_base_url=openai_base_url or DEFAULT_OPENAI_BASE_URL,
        openai_model=openai_model or DEFAULT_OPENAI_MODEL,
        rerank_type=rerank or DEFAULT_RERANK_TYPE,  # type: ignore[arg-type]
        rerank_model=rerank_model or DEFAULT_RERANK_MODEL,
        rerank_top_k=rerank_top_k or DEFAULT_RERANK_TOP_K,
        rerank_initial_k=rerank_initial_k or DEFAULT_RERANK_INITIAL_K,
        chunk_max_size=chunk_size or DEFAULT_CHUNK_MAX_SIZE,
        data_dir=data_dir or _get_default_data_dir(),
        prebuilt_url=prebuilt_url,
        http_host=http_host or DEFAULT_HTTP_HOST,
        http_port=http_port or DEFAULT_HTTP_PORT,
        indexes=indexes,
    )
    set_settings(settings)
    return settings


@app.callback(invoke_without_command=True)
def main(
    ctx: typer.Context,
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
            help="Local HuggingFace embedding model name",
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
    rerank: Annotated[
        str | None,
        typer.Option(
            "--rerank",
            "-r",
            help="Rerank type (none/local/openai)",
            envvar="CANGJIE_RERANK_TYPE",
        ),
    ] = None,
    rerank_model: Annotated[
        str | None,
        typer.Option(
            "--rerank-model",
            help="Rerank model name",
            envvar="CANGJIE_RERANK_MODEL",
        ),
    ] = None,
    rerank_top_k: Annotated[
        int | None,
        typer.Option(
            "--rerank-top-k",
            help="Number of results after reranking",
            envvar="CANGJIE_RERANK_TOP_K",
        ),
    ] = None,
    rerank_initial_k: Annotated[
        int | None,
        typer.Option(
            "--rerank-initial-k",
            help="Number of candidates before reranking",
            envvar="CANGJIE_RERANK_INITIAL_K",
        ),
    ] = None,
    chunk_size: Annotated[
        int | None,
        typer.Option(
            "--chunk-size",
            help="Max chunk size in characters",
            envvar="CANGJIE_CHUNK_MAX_SIZE",
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
    """Start the MCP server in stdio mode (default).

    When invoked without a subcommand, starts the MCP server using stdio transport.
    This is the default behavior for MCP client integration.

    For HTTP server mode, use: cangjie-mcp serve
    """
    # If a subcommand is invoked, let it handle execution
    if ctx.invoked_subcommand is not None:
        return

    settings = _create_settings(
        version=version,
        lang=lang,
        embedding=embedding,
        local_model=local_model,
        openai_api_key=openai_api_key,
        openai_base_url=openai_base_url,
        openai_model=openai_model,
        rerank=rerank,
        rerank_model=rerank_model,
        rerank_top_k=rerank_top_k,
        rerank_initial_k=rerank_initial_k,
        chunk_size=chunk_size,
        data_dir=data_dir,
    )

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


@app.command()
def serve(
    indexes: Annotated[
        str | None,
        typer.Option(
            "--indexes",
            "-i",
            help="Comma-separated list of URLs to prebuilt index archives",
            envvar="CANGJIE_INDEXES",
        ),
    ] = None,
    host: Annotated[
        str | None,
        typer.Option(
            "--host",
            "-H",
            help="HTTP server host address",
            envvar="CANGJIE_HTTP_HOST",
        ),
    ] = None,
    port: Annotated[
        int | None,
        typer.Option(
            "--port",
            "-p",
            help="HTTP server port",
            envvar="CANGJIE_HTTP_PORT",
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
            help="Local HuggingFace embedding model name",
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
    rerank: Annotated[
        str | None,
        typer.Option(
            "--rerank",
            "-r",
            help="Rerank type (none/local/openai)",
            envvar="CANGJIE_RERANK_TYPE",
        ),
    ] = None,
    rerank_model: Annotated[
        str | None,
        typer.Option(
            "--rerank-model",
            help="Rerank model name",
            envvar="CANGJIE_RERANK_MODEL",
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
    """Start the HTTP server with multi-index support.

    Downloads prebuilt indexes from URLs and serves them via HTTP.
    The version and language are derived from each archive's metadata.
    Archives are cached locally using the URL's MD5 hash.

    Examples:
        # Load single index from URL
        cangjie-mcp serve --indexes "https://example.com/cangjie-index-v1-zh.tar.gz"

        # Load multiple indexes
        cangjie-mcp serve --indexes "https://example.com/v1-zh.tar.gz,https://example.com/v2-en.tar.gz"

        # Access via HTTP (routes derived from archive metadata):
        # POST http://localhost:8000/v1/zh/mcp    -> v1 Chinese docs
        # POST http://localhost:8000/v2/en/mcp   -> v2 English docs
    """
    from cangjie_mcp.indexer.multi_store import parse_index_urls

    settings = _create_settings(
        embedding=embedding,
        local_model=local_model,
        openai_api_key=openai_api_key,
        openai_base_url=openai_base_url,
        openai_model=openai_model,
        rerank=rerank,
        rerank_model=rerank_model,
        data_dir=data_dir,
        http_host=host,
        http_port=port,
        indexes=indexes,
    )

    # Parse index URLs
    indexes_str = settings.indexes
    if not indexes_str:
        console.print("[red]No indexes specified. Use --indexes or CANGJIE_INDEXES.[/red]")
        console.print(
            "[yellow]Example: cangjie-mcp serve --indexes 'https://example.com/cangjie-index-v1-zh.tar.gz'[/yellow]"
        )
        raise typer.Exit(1)

    index_urls = parse_index_urls(indexes_str)
    if not index_urls:
        console.print(f"[red]No valid URLs in: {indexes_str}[/red]")
        raise typer.Exit(1)

    console.print("[bold]Cangjie MCP HTTP Server[/bold]")
    console.print(f"  Host: {settings.http_host}")
    console.print(f"  Port: {settings.http_port}")
    console.print(f"  Embedding: {settings.embedding_type}")
    console.print(f"  Rerank: {settings.rerank_type}")
    console.print(f"  Index URLs: {len(index_urls)}")
    for url in index_urls:
        console.print(f"    - {url}")
    console.print()

    # Start HTTP server
    from cangjie_mcp.server.http import MultiIndexHTTPServer

    server = MultiIndexHTTPServer(settings=settings, index_urls=index_urls)

    try:
        server.run()
    except KeyboardInterrupt:
        console.print("\n[yellow]Server stopped.[/yellow]")


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
    actual_data_dir = data_dir or _get_default_data_dir()

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

    settings = _create_settings(
        version=version,
        lang=lang,
        embedding=embedding,
        local_model=local_model,
        openai_api_key=openai_api_key,
        openai_base_url=openai_base_url,
        openai_model=openai_model,
        chunk_size=chunk_size,
        data_dir=data_dir,
    )

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

    actual_data_dir = data_dir or _get_default_data_dir()
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
