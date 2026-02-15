"""Index initialization and building logic.

This module provides functions for initializing the documentation index,
checking for prebuilt indexes, and building new indexes when needed.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from cangjie_mcp.utils import logger

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
    logger.info("Ensuring documentation repository...")
    git_mgr = GitManager(settings.docs_repo_dir)
    git_mgr.ensure_cloned()

    current_version = git_mgr.get_current_version()
    if current_version != settings.docs_version:
        logger.info("Checking out version %s...", settings.docs_version)
        git_mgr.checkout(settings.docs_version)

    # Load documents
    logger.info("Loading documents...")
    loader = DocumentLoader(settings.docs_source_dir)
    documents = loader.load_all_documents()

    if not documents:
        import typer

        typer.echo("No documents found!")
        raise typer.Exit(1)

    logger.info("Loaded %d documents", len(documents))

    # Chunk documents
    logger.info("Chunking documents...")
    chunker = create_chunker(embedding_provider, max_chunk_size=settings.chunk_max_size)
    nodes = chunker.chunk_documents(documents, use_semantic=True)
    logger.info("Created %d chunks", len(nodes))

    # Index
    logger.info("Building index...")
    store.index_nodes(nodes)
    store.save_metadata(
        version=settings.docs_version,
        lang=settings.docs_lang,
        embedding_model=embedding_provider.get_model_name(),
    )

    logger.info("Index built successfully!")


def initialize_and_index(settings: Settings) -> None:
    """Initialize repository and build index if needed.

    This function:
    1. Checks for a matching prebuilt index
    2. If not found, checks for an existing index with matching version/lang
    3. If neither exists, clones the repo and builds a new index

    Args:
        settings: Application settings with paths and configuration
    """
    from cangjie_mcp.prebuilt.manager import PrebuiltManager

    prebuilt_mgr = PrebuiltManager(settings.data_dir)

    # When prebuilt_url is configured, version/lang/embedding are determined by the archive
    if settings.prebuilt_url:
        _warn_ignored_settings(settings)

        installed = prebuilt_mgr.get_installed_metadata()
        if installed:
            logger.info("Using prebuilt index (version: %s, lang: %s)", installed.version, installed.lang)
            return

        archive = prebuilt_mgr.download(settings.prebuilt_url)
        prebuilt_mgr.install(archive)
        return

    # No prebuilt URL — use version/lang to check existing index
    from cangjie_mcp.indexer.embeddings import get_embedding_provider
    from cangjie_mcp.indexer.store import create_vector_store

    installed = prebuilt_mgr.get_installed_metadata()
    if installed and installed.version == settings.docs_version and installed.lang == settings.docs_lang:
        logger.info("Using prebuilt index (version: %s, lang: %s)", settings.docs_version, settings.docs_lang)
        return

    store = create_vector_store(settings, with_rerank=False)

    if store.is_indexed() and store.version_matches(settings.docs_version, settings.docs_lang):
        logger.info("Index already exists (version: %s, lang: %s)", settings.docs_version, settings.docs_lang)
        return

    # Need to build index
    embedding_provider = get_embedding_provider(settings)
    build_index(settings, store, embedding_provider)


_PREBUILT_IGNORED_SETTINGS = ("docs_version", "docs_lang", "embedding_type", "local_model")


def _warn_ignored_settings(settings: Settings) -> None:
    """Warn about settings that are ignored when prebuilt_url is set."""
    from cangjie_mcp.defaults import (
        DEFAULT_DOCS_LANG,
        DEFAULT_DOCS_VERSION,
        DEFAULT_EMBEDDING_TYPE,
        DEFAULT_LOCAL_MODEL,
    )

    defaults = {
        "docs_version": DEFAULT_DOCS_VERSION,
        "docs_lang": DEFAULT_DOCS_LANG,
        "embedding_type": DEFAULT_EMBEDDING_TYPE,
        "local_model": DEFAULT_LOCAL_MODEL,
    }

    overridden = [name for name in _PREBUILT_IGNORED_SETTINGS if getattr(settings, name) != defaults[name]]

    if overridden:
        names = ", ".join(f"--{name.replace('_', '-')}" for name in overridden)
        logger.warning(
            "prebuilt_url is set, %s will be ignored — these values are determined by the prebuilt archive.",
            names,
        )


def print_settings_summary(settings: Settings) -> None:
    """Print a summary of the current settings.

    Args:
        settings: Application settings to summarize
    """
    logger.info(
        "Cangjie MCP Server — version=%s, lang=%s, embedding=%s, rerank=%s%s",
        settings.docs_version,
        settings.docs_lang,
        settings.embedding_type,
        settings.rerank_type,
        f", rerank_model={settings.rerank_model}" if settings.rerank_type != "none" else "",
    )
