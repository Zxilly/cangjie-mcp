"""Index initialization and building logic.

This module provides functions for initializing the documentation index,
checking for prebuilt indexes, and building new indexes when needed.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from cangjie_mcp.utils import console

if TYPE_CHECKING:
    from cangjie_mcp.config import Settings
    from cangjie_mcp.indexer.embeddings import EmbeddingProvider
    from cangjie_mcp.indexer.store import VectorStore


def build_index(settings: Settings, store: VectorStore, embedding_provider: EmbeddingProvider) -> None:
    """Build the vector index from a git documentation repository.

    Ensures the repo is cloned and at the correct version, loads documents,
    chunks them, indexes into the store, and saves metadata.

    Args:
        settings: Application settings with paths and configuration
        store: VectorStore instance to index into
        embedding_provider: EmbeddingProvider for chunking

    Raises:
        typer.Exit: If no documents are found
    """
    from cangjie_mcp.indexer.chunker import create_chunker
    from cangjie_mcp.indexer.loader import DocumentLoader
    from cangjie_mcp.repo.git_manager import GitManager

    # Ensure repo is ready
    console.print("[blue]Ensuring documentation repository...[/blue]")
    git_mgr = GitManager(settings.docs_repo_dir)
    git_mgr.ensure_cloned()

    current_version = git_mgr.get_current_version()
    if current_version != settings.docs_version:
        console.print(f"[blue]Checking out version {settings.docs_version}...[/blue]")
        git_mgr.checkout(settings.docs_version)

    # Load documents
    console.print("[blue]Loading documents...[/blue]")
    loader = DocumentLoader(settings.docs_source_dir)
    documents = loader.load_all_documents()

    if not documents:
        console.print("[red]No documents found![/red]")
        import typer

        raise typer.Exit(1)

    console.print(f"  Loaded {len(documents)} documents")

    # Chunk documents
    console.print("[blue]Chunking documents...[/blue]")
    chunker = create_chunker(embedding_provider, max_chunk_size=settings.chunk_max_size)
    nodes = chunker.chunk_documents(documents, use_semantic=True)
    console.print(f"  Created {len(nodes)} chunks")

    # Index
    console.print("[blue]Building index...[/blue]")
    store.index_nodes(nodes)
    store.save_metadata(
        version=settings.docs_version,
        lang=settings.docs_lang,
        embedding_model=embedding_provider.get_model_name(),
    )

    console.print("[green]Index built successfully![/green]")


def initialize_and_index(settings: Settings) -> None:
    """Initialize repository and build index if needed.

    This function:
    1. Checks for a matching prebuilt index
    2. If not found, checks for an existing index with matching version/lang
    3. If neither exists, clones the repo and builds a new index

    Args:
        settings: Application settings with paths and configuration
    """
    from cangjie_mcp.indexer.embeddings import get_embedding_provider
    from cangjie_mcp.indexer.store import create_vector_store
    from cangjie_mcp.prebuilt.manager import PrebuiltManager

    # Check for prebuilt index first
    prebuilt_mgr = PrebuiltManager(settings.data_dir)
    installed = prebuilt_mgr.get_installed_metadata()

    if installed and installed.version == settings.docs_version and installed.lang == settings.docs_lang:
        console.print(
            f"[green]Using prebuilt index (version: {settings.docs_version}, lang: {settings.docs_lang})[/green]"
        )
        return

    # Auto-download prebuilt index if URL is configured
    if settings.prebuilt_url:
        console.print(f"[blue]Downloading prebuilt index from {settings.prebuilt_url}...[/blue]")
        archive = prebuilt_mgr.download(settings.prebuilt_url, settings.docs_version, settings.docs_lang)
        prebuilt_mgr.install(archive)
        console.print(
            f"[green]Prebuilt index installed (version: {settings.docs_version}, lang: {settings.docs_lang})[/green]"
        )
        return

    # Check existing index
    store = create_vector_store(settings, with_rerank=False)

    if store.is_indexed() and store.version_matches(settings.docs_version, settings.docs_lang):
        console.print(
            f"[green]Index already exists (version: {settings.docs_version}, lang: {settings.docs_lang})[/green]"
        )
        return

    # Need to build index
    embedding_provider = get_embedding_provider(settings)
    build_index(settings, store, embedding_provider)


def print_settings_summary(settings: Settings) -> None:
    """Print a summary of the current settings.

    Args:
        settings: Application settings to summarize
    """
    console.print("[bold]Cangjie MCP Server[/bold]")
    console.print(f"  Version: {settings.docs_version}")
    console.print(f"  Language: {settings.docs_lang}")
    console.print(f"  Embedding: {settings.embedding_type}")
    console.print(f"  Rerank: {settings.rerank_type}")
    if settings.rerank_type != "none":
        console.print(f"  Rerank Model: {settings.rerank_model}")
    console.print()
