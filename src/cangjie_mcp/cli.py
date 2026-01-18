"""CLI for Cangjie MCP server."""

from pathlib import Path
from typing import Annotated

import typer
from rich.console import Console
from rich.table import Table

from cangjie_mcp.config import Settings, get_settings, update_settings

app = typer.Typer(
    name="cangjie-mcp",
    help="MCP server for Cangjie programming language documentation",
)

prebuilt_app = typer.Typer(help="Prebuilt index management commands")
app.add_typer(prebuilt_app, name="prebuilt")

console = Console()


def initialize_and_index(settings: Settings) -> None:
    """Initialize repository and build index if needed."""
    from cangjie_mcp.indexer.chunker import create_chunker
    from cangjie_mcp.indexer.embeddings import get_embedding_provider
    from cangjie_mcp.indexer.loader import DocumentLoader
    from cangjie_mcp.indexer.store import VectorStore
    from cangjie_mcp.prebuilt.manager import PrebuiltManager
    from cangjie_mcp.repo.git_manager import GitManager

    # Check for prebuilt index first
    prebuilt_mgr = PrebuiltManager(settings.data_dir)
    installed = prebuilt_mgr.get_installed_metadata()

    if (
        installed
        and installed.version == settings.docs_version
        and installed.lang == settings.docs_lang
    ):
        console.print(
            f"[green]Using prebuilt index (version: {settings.docs_version}, "
            f"lang: {settings.docs_lang})[/green]"
        )
        return

    # Check existing index
    embedding_provider = get_embedding_provider(settings)
    store = VectorStore(
        db_path=settings.chroma_db_dir,
        embedding_provider=embedding_provider,
    )

    if store.is_indexed() and store.version_matches(settings.docs_version, settings.docs_lang):
        console.print(
            f"[green]Index already exists (version: {settings.docs_version}, "
            f"lang: {settings.docs_lang})[/green]"
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
    chunker = create_chunker(embedding_provider)
    nodes = chunker.chunk_documents(documents, use_semantic=True)

    # Index
    store.index_nodes(nodes)
    store.save_metadata(
        version=settings.docs_version,
        lang=settings.docs_lang,
        embedding_model=embedding_provider.get_model_name(),
    )

    console.print("[green]Index built successfully![/green]")


@app.command()
def serve(
    version: Annotated[
        str | None,
        typer.Option("--version", "-v", help="Documentation version (git tag)"),
    ] = None,
    lang: Annotated[
        str | None,
        typer.Option("--lang", "-l", help="Documentation language (zh/en)"),
    ] = None,
    embedding: Annotated[
        str | None,
        typer.Option("--embedding", "-e", help="Embedding type (local/openai)"),
    ] = None,
    transport: Annotated[
        str,
        typer.Option("--transport", "-t", help="Transport type (stdio/http)"),
    ] = "stdio",
    port: Annotated[
        int,
        typer.Option("--port", "-p", help="HTTP port (only for http transport)"),
    ] = 8000,
) -> None:
    """Start the MCP server.

    Automatically initializes documentation repository and builds index if needed.
    """
    # Update settings with CLI overrides
    overrides = {
        "docs_version": version,
        "docs_lang": lang,
        "embedding_type": embedding,
    }
    settings = update_settings(**{k: v for k, v in overrides.items() if v})

    console.print("[bold]Cangjie MCP Server[/bold]")
    console.print(f"  Version: {settings.docs_version}")
    console.print(f"  Language: {settings.docs_lang}")
    console.print(f"  Embedding: {settings.embedding_type}")
    console.print(f"  Transport: {transport}")
    console.print()

    # Initialize and index
    initialize_and_index(settings)

    # Start server
    from cangjie_mcp.server.app import create_mcp_server

    mcp = create_mcp_server(settings)

    if transport == "stdio":
        console.print("[blue]Starting MCP server (stdio)...[/blue]")
        mcp.run(transport="stdio")
    else:
        console.print(f"[blue]Starting MCP server (sse on port {port})...[/blue]")
        mcp.run(transport="sse")


@prebuilt_app.command("download")
def prebuilt_download(
    url: Annotated[
        str | None,
        typer.Option("--url", "-u", help="URL to download from"),
    ] = None,
    version: Annotated[
        str | None,
        typer.Option("--version", "-v", help="Version to download"),
    ] = None,
    lang: Annotated[
        str | None,
        typer.Option("--lang", "-l", help="Language to download"),
    ] = None,
) -> None:
    """Download a prebuilt index."""
    from cangjie_mcp.prebuilt.manager import PrebuiltManager

    settings = get_settings()

    if not url:
        url = settings.prebuilt_url
        if not url:
            console.print("[red]No URL provided. Set CANGJIE_PREBUILT_URL or use --url[/red]")
            raise typer.Exit(1)

    version = version or settings.docs_version
    lang = lang or settings.docs_lang

    mgr = PrebuiltManager(settings.data_dir)
    try:
        archive_path = mgr.download(url, version, lang)
        mgr.install(archive_path)
    except Exception as e:
        console.print(f"[red]Failed to download: {e}[/red]")
        raise typer.Exit(1) from None


@prebuilt_app.command("build")
def prebuilt_build(
    version: Annotated[
        str | None,
        typer.Option("--version", "-v", help="Documentation version (git tag)"),
    ] = None,
    lang: Annotated[
        str | None,
        typer.Option("--lang", "-l", help="Documentation language (zh/en)"),
    ] = None,
    embedding: Annotated[
        str | None,
        typer.Option("--embedding", "-e", help="Embedding type (local/openai)"),
    ] = None,
    embedding_model: Annotated[
        str | None,
        typer.Option("--embedding-model", "-m", help="Embedding model name"),
    ] = None,
    data_dir: Annotated[
        Path | None,
        typer.Option("--data-dir", "-d", help="Data directory"),
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

    # Update settings with CLI overrides
    overrides: dict[str, str | Path] = {}
    if version:
        overrides["docs_version"] = version
    if lang:
        overrides["docs_lang"] = lang
    if embedding:
        overrides["embedding_type"] = embedding
    if data_dir:
        overrides["data_dir"] = data_dir

    settings = update_settings(**overrides)

    console.print("[bold]Building Prebuilt Index Archive[/bold]")
    console.print(f"  Version: {settings.docs_version}")
    console.print(f"  Language: {settings.docs_lang}")
    console.print(f"  Embedding: {settings.embedding_type}")
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
    embedding_provider = create_embedding_provider(settings, model_override=embedding_model)
    chunker = create_chunker(embedding_provider)
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
    mgr = PrebuiltManager(settings.data_dir)

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
def prebuilt_list() -> None:
    """List available prebuilt indexes."""
    from cangjie_mcp.prebuilt.manager import PrebuiltManager

    settings = get_settings()
    mgr = PrebuiltManager(settings.data_dir)

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

    # Show installed
    installed = mgr.get_installed_metadata()
    if installed:
        console.print()
        console.print("[bold]Currently Installed:[/bold]")
        console.print(f"  Version: {installed.version}")
        console.print(f"  Language: {installed.lang}")
        console.print(f"  Embedding: {installed.embedding_model}")


if __name__ == "__main__":
    app()
