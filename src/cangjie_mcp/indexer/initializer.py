"""Index initialization and building logic.

This module provides functions for initializing the documentation index
and building new indexes when needed.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from cangjie_mcp.utils import logger

if TYPE_CHECKING:
    from cangjie_mcp.config import IndexInfo, Settings
    from cangjie_mcp.indexer.embeddings import EmbeddingProvider
    from cangjie_mcp.indexer.store import VectorStore


def build_index(
    settings: Settings, index_info: IndexInfo, store: VectorStore, embedding_provider: EmbeddingProvider
) -> None:
    """Build the vector index from a git documentation repository.

    Ensures the repo is cloned and at the correct version, loads documents,
    chunks them, indexes into the store, and saves metadata.

    Args:
        settings: Application settings (CLI configuration like chunk_max_size)
        index_info: Index identity and paths
        store: VectorStore instance to index into
        embedding_provider: EmbeddingProvider for chunking

    Raises:
        RuntimeError: If no documents are found
    """
    from cangjie_mcp.indexer.chunker import create_chunker
    from cangjie_mcp.indexer.loader import DocumentLoader
    from cangjie_mcp.repo.git_manager import GitManager

    # Ensure repo is ready
    logger.info("Ensuring documentation repository...")
    git_mgr = GitManager(index_info.docs_repo_dir)
    git_mgr.ensure_cloned()

    current_version = git_mgr.get_current_version()
    if current_version != index_info.version:
        logger.info("Checking out version %s...", index_info.version)
        git_mgr.checkout(index_info.version)

    # Load documents
    logger.info("Loading documents...")
    loader = DocumentLoader(index_info.docs_source_dir)
    documents = loader.load_all_documents()

    if not documents:
        raise RuntimeError(f"No documents found in {index_info.docs_source_dir}")

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
        version=index_info.version,
        lang=index_info.lang,
        embedding_model=embedding_provider.get_model_name(),
    )

    logger.info("Index built successfully!")


def _index_is_ready(index_info: IndexInfo, version: str, lang: str) -> bool:
    """Check if a valid index exists by reading the metadata file.

    This avoids creating a ChromaDB client or loading the embedding model,
    making it much cheaper than creating a full VectorStore just to check.
    """
    from cangjie_mcp.indexer.store import METADATA_FILE, IndexMetadata

    metadata_path = index_info.chroma_db_dir / METADATA_FILE
    if not metadata_path.exists():
        return False
    try:
        metadata = IndexMetadata.model_validate_json(metadata_path.read_text(encoding="utf-8"))
        return metadata.version == version and metadata.lang == lang and metadata.document_count > 0
    except Exception:
        return False


def initialize_and_index(settings: Settings) -> IndexInfo:
    """Initialize repository and build index if needed.

    This function:
    1. Checks for an existing index with matching version/lang
    2. If none exists, clones the repo and builds a new index

    Args:
        settings: Application settings with paths and configuration

    Returns:
        IndexInfo describing the active index
    """
    from cangjie_mcp.config import IndexInfo as IndexInfoCls

    index_info = IndexInfoCls.from_settings(settings)

    if _index_is_ready(index_info, settings.docs_version, settings.docs_lang):
        logger.info("Index already exists (version: %s, lang: %s)", settings.docs_version, settings.docs_lang)
        return index_info

    # Need to build index
    from cangjie_mcp.indexer.embeddings import get_embedding_provider
    from cangjie_mcp.indexer.store import VectorStore

    embedding_provider = get_embedding_provider(settings)
    store = VectorStore(
        db_path=index_info.chroma_db_dir,
        embedding_provider=embedding_provider,
    )
    build_index(settings, index_info, store, embedding_provider)
    return index_info
