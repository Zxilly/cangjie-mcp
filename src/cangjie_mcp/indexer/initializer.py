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
    settings: Settings,
    index_info: IndexInfo,
    store: VectorStore | None,
    embedding_provider: EmbeddingProvider | None,
) -> None:
    """Build the index from documentation already checked-out on disk.

    Always builds a BM25 index. When an embedding provider and vector store
    are supplied, also builds a vector index.

    The caller (``initialize_and_index``) is responsible for cloning the
    repository and checking out the correct version **before** calling this
    function.

    Args:
        settings: Application settings (CLI configuration like chunk_max_size)
        index_info: Index identity and paths
        store: VectorStore instance to index into (None in BM25-only mode)
        embedding_provider: EmbeddingProvider for chunking (None in BM25-only mode)

    Raises:
        RuntimeError: If no documents are found
    """
    from cangjie_mcp.indexer.bm25_store import BM25Store
    from cangjie_mcp.indexer.chunker import create_chunker
    from cangjie_mcp.indexer.loader import DocumentLoader
    from cangjie_mcp.indexer.store import IndexMetadata

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
    use_semantic = embedding_provider is not None
    nodes = chunker.chunk_documents(documents, use_semantic=use_semantic)
    logger.info("Created %d chunks", len(nodes))

    # Build BM25 index (always)
    logger.info("Building BM25 index...")
    bm25_store = BM25Store(index_info.bm25_index_dir)
    bm25_store.build_from_nodes(nodes)

    # Build vector index (when embedding is configured)
    if store is not None and embedding_provider is not None:
        logger.info("Building vector index...")
        store.index_nodes(nodes)
        store.save_metadata(
            version=index_info.version,
            lang=index_info.lang,
            embedding_model=embedding_provider.get_model_name(),
        )

    # Write top-level index metadata
    search_mode = "hybrid" if settings.has_embedding else "bm25"
    metadata = IndexMetadata(
        version=index_info.version,
        lang=index_info.lang,
        embedding_model=settings.embedding_model_name,
        document_count=len(nodes),
        search_mode=search_mode,
    )
    metadata_path = index_info.index_dir / "index_metadata.json"
    metadata_path.parent.mkdir(parents=True, exist_ok=True)
    metadata_path.write_text(metadata.model_dump_json(indent=2), encoding="utf-8")

    logger.info("Index built successfully!")


def _index_is_ready(index_info: IndexInfo, version: str, lang: str, has_embedding: bool) -> bool:
    """Check if a valid index exists by reading the metadata file.

    Always checks for BM25 index existence. When has_embedding is True,
    also checks for the ChromaDB metadata file.
    """
    from cangjie_mcp.indexer.store import METADATA_FILE, IndexMetadata

    # Check BM25 index
    if not index_info.bm25_index_dir.exists():
        return False

    # Check vector index (when embedding is configured)
    if has_embedding:
        metadata_path = index_info.chroma_db_dir / METADATA_FILE
        if not metadata_path.exists():
            return False
        try:
            metadata = IndexMetadata.model_validate_json(metadata_path.read_text(encoding="utf-8"))
            return metadata.version == version and metadata.lang == lang and metadata.document_count > 0
        except Exception:
            return False

    # BM25-only: check top-level metadata
    top_metadata_path = index_info.index_dir / "index_metadata.json"
    if not top_metadata_path.exists():
        return False
    try:
        metadata = IndexMetadata.model_validate_json(top_metadata_path.read_text(encoding="utf-8"))
        return metadata.version == version and metadata.lang == lang and metadata.document_count > 0
    except Exception:
        return False


def initialize_and_index(settings: Settings) -> IndexInfo:
    """Initialize repository and build index if needed.

    This function:
    1. Clones / fetches the documentation repository
    2. Resolves the requested version (branches become ``branch(hash)``)
    3. Checks for an existing index with matching resolved version
    4. Builds a new index if none exists

    Args:
        settings: Application settings with paths and configuration

    Returns:
        IndexInfo describing the active index
    """
    from cangjie_mcp.config import IndexInfo as IndexInfoCls
    from cangjie_mcp.repo.git_manager import GitManager

    # Resolve version (ensures repo is cloned, fetched, and checked out)
    git_mgr = GitManager(settings.docs_repo_dir)
    resolved_version = git_mgr.resolve_version(settings.docs_version)
    logger.info("Resolved version: %s -> %s", settings.docs_version, resolved_version)

    index_info = IndexInfoCls(
        version=resolved_version,
        lang=settings.docs_lang,
        embedding_model_name=settings.embedding_model_name,
        data_dir=settings.data_dir,
    )

    if _index_is_ready(index_info, resolved_version, settings.docs_lang, settings.has_embedding):
        logger.info("Index already exists (version: %s, lang: %s)", resolved_version, settings.docs_lang)
        return index_info

    # Need to build index — create providers only when embedding is configured
    embedding_provider: EmbeddingProvider | None = None
    store: VectorStore | None = None

    if settings.has_embedding:
        from cangjie_mcp.indexer.embeddings import get_embedding_provider
        from cangjie_mcp.indexer.store import VectorStore as VectorStoreCls

        embedding_provider = get_embedding_provider(settings)
        store = VectorStoreCls(
            db_path=index_info.chroma_db_dir,
            embedding_provider=embedding_provider,
        )

    build_index(settings, index_info, store, embedding_provider)
    return index_info
