"""ChromaDB vector store for document indexing and retrieval."""

from __future__ import annotations

import contextlib
from pathlib import Path
from typing import TYPE_CHECKING

import chromadb
from chromadb.config import Settings as ChromaSettings
from llama_index.core import Document, StorageContext, VectorStoreIndex
from llama_index.core.schema import BaseNode
from llama_index.vector_stores.chroma import ChromaVectorStore
from pydantic import BaseModel

from cangjie_mcp.indexer.embeddings import EmbeddingProvider
from cangjie_mcp.indexer.reranker import RerankerProvider
from cangjie_mcp.utils import logger

if TYPE_CHECKING:
    from chromadb.api import ClientAPI
    from chromadb.api.models.Collection import Collection

    from cangjie_mcp.config import Settings

# Metadata file for version tracking
METADATA_FILE = "index_metadata.json"


class IndexMetadata(BaseModel):
    """Index metadata structure."""

    version: str
    lang: str
    embedding_model: str
    document_count: int


class SearchResultMetadata(BaseModel):
    """Metadata from search result."""

    file_path: str = ""
    category: str = ""
    topic: str = ""
    title: str = ""

    @classmethod
    def from_node_metadata(cls, metadata: dict[str, str]) -> SearchResultMetadata:
        """Create SearchResultMetadata from node metadata dict.

        Args:
            metadata: Node metadata dictionary

        Returns:
            SearchResultMetadata instance
        """
        return cls(
            file_path=str(metadata.get("file_path", "")),
            category=str(metadata.get("category", "")),
            topic=str(metadata.get("topic", "")),
            title=str(metadata.get("title", "")),
        )


class SearchResult(BaseModel):
    """Search result structure."""

    text: str
    score: float
    metadata: SearchResultMetadata


class VectorStore:
    """ChromaDB-based vector store for Cangjie documentation."""

    def __init__(
        self,
        db_path: Path,
        embedding_provider: EmbeddingProvider,
        collection_name: str = "cangjie_docs",
        reranker: RerankerProvider | None = None,
    ) -> None:
        """Initialize vector store.

        Args:
            db_path: Path to ChromaDB storage directory
            embedding_provider: Embedding provider for vectorization
            collection_name: Name of the ChromaDB collection
            reranker: Optional reranker provider for result reranking
        """
        self.db_path = db_path
        self.embedding_provider = embedding_provider
        self.collection_name = collection_name
        self.reranker = reranker
        self._client: ClientAPI | None = None
        self._collection: Collection | None = None
        self._index: VectorStoreIndex | None = None

    @property
    def client(self) -> ClientAPI:
        """Get or create ChromaDB client."""
        if self._client is None:
            self.db_path.mkdir(parents=True, exist_ok=True)
            self._client = chromadb.PersistentClient(
                path=str(self.db_path),
                settings=ChromaSettings(anonymized_telemetry=False),
            )
        return self._client

    @property
    def collection(self) -> Collection:
        """Get or create ChromaDB collection."""
        if self._collection is None:
            self._collection = self.client.get_or_create_collection(
                name=self.collection_name,
            )
        return self._collection

    def is_indexed(self) -> bool:
        """Check if documents are already indexed."""
        return self.db_path.exists() and self.collection.count() > 0

    def get_metadata(self) -> IndexMetadata | None:
        """Get stored index metadata.

        Returns:
            Metadata dict or None if not found
        """
        metadata_path = self.db_path / METADATA_FILE
        if metadata_path.exists():
            return IndexMetadata.model_validate_json(metadata_path.read_text(encoding="utf-8"))
        return None

    def save_metadata(self, version: str, lang: str, embedding_model: str) -> None:
        """Save index metadata.

        Args:
            version: Documentation version
            lang: Documentation language
            embedding_model: Name of embedding model used
        """
        metadata = IndexMetadata(
            version=version,
            lang=lang,
            embedding_model=embedding_model,
            document_count=self.collection.count(),
        )
        metadata_path = self.db_path / METADATA_FILE
        metadata_path.write_text(metadata.model_dump_json(indent=2), encoding="utf-8")

    def version_matches(self, version: str, lang: str) -> bool:
        """Check if indexed version matches requested version.

        Args:
            version: Requested version
            lang: Requested language

        Returns:
            True if versions match
        """
        metadata = self.get_metadata()
        if metadata is None:
            return False
        return metadata.version == version and metadata.lang == lang

    def _reset_collection(self) -> StorageContext:
        """Clear and recreate the collection, returning a storage context."""
        with contextlib.suppress(Exception):
            self.client.delete_collection(self.collection_name)

        self._collection = self.client.create_collection(name=self.collection_name)
        vector_store = ChromaVectorStore(chroma_collection=self.collection)
        return StorageContext.from_defaults(vector_store=vector_store)

    def index_nodes(self, nodes: list[BaseNode]) -> VectorStoreIndex:
        """Index text nodes into ChromaDB.

        Args:
            nodes: List of text nodes to index

        Returns:
            VectorStoreIndex for querying
        """
        logger.info("Indexing %d nodes into ChromaDB...", len(nodes))

        storage_context = self._reset_collection()
        embed_model = self.embedding_provider.get_embedding_model()

        self._index = VectorStoreIndex(
            nodes=nodes,
            storage_context=storage_context,
            embed_model=embed_model,
            show_progress=True,
        )

        logger.info("Indexing complete.")
        return self._index

    def index_documents(self, documents: list[Document]) -> VectorStoreIndex:
        """Index documents directly (uses default chunking).

        Args:
            documents: List of documents to index

        Returns:
            VectorStoreIndex for querying
        """
        logger.info("Indexing %d documents into ChromaDB...", len(documents))

        storage_context = self._reset_collection()
        embed_model = self.embedding_provider.get_embedding_model()

        self._index = VectorStoreIndex.from_documents(
            documents=documents,
            storage_context=storage_context,
            embed_model=embed_model,
            show_progress=True,
        )

        logger.info("Indexing complete.")
        return self._index

    def get_index(self) -> VectorStoreIndex | None:
        """Get the vector store index for querying.

        Returns:
            VectorStoreIndex or None if not indexed
        """
        if self._index is not None:
            return self._index

        if not self.is_indexed():
            return None

        # Load existing index
        vector_store = ChromaVectorStore(chroma_collection=self.collection)
        embed_model = self.embedding_provider.get_embedding_model()
        index: VectorStoreIndex = VectorStoreIndex.from_vector_store(
            vector_store=vector_store,
            embed_model=embed_model,
        )
        self._index = index
        return self._index

    def search(
        self,
        query: str,
        top_k: int = 5,
        category: str | None = None,
        use_rerank: bool = True,
        initial_k: int | None = None,
    ) -> list[SearchResult]:
        """Search for documents matching query.

        Args:
            query: Search query
            top_k: Number of results to return
            category: Optional category filter
            use_rerank: Whether to use reranking (if reranker is available)
            initial_k: Number of candidates to retrieve before reranking.
                       If None, uses config default or top_k * 4.

        Returns:
            List of search results with text and metadata
        """
        index = self.get_index()
        if index is None:
            return []

        # Determine how many candidates to retrieve
        should_rerank = use_rerank and self.reranker is not None
        if should_rerank:  # noqa: SIM108
            # Retrieve more candidates for reranking
            retrieve_k = initial_k if initial_k is not None else max(top_k * 4, 20)
        else:
            retrieve_k = top_k

        # Build retriever with filters
        filters = None
        if category:
            from llama_index.core.vector_stores import MetadataFilter, MetadataFilters

            filters = MetadataFilters(filters=[MetadataFilter(key="category", value=category)])

        retriever = index.as_retriever(
            similarity_top_k=retrieve_k,
            filters=filters,
        )

        nodes = retriever.retrieve(query)

        # Apply reranking if enabled
        if should_rerank and self.reranker is not None:
            nodes = self.reranker.rerank(query=query, nodes=nodes, top_k=top_k)

        results: list[SearchResult] = []
        for node in nodes[:top_k]:
            results.append(
                SearchResult(
                    text=node.text,
                    score=node.score if node.score is not None else 0.0,
                    metadata=SearchResultMetadata.from_node_metadata(node.metadata),
                )
            )

        return results

    def clear(self) -> None:
        """Clear all indexed data."""
        try:
            self.client.delete_collection(self.collection_name)
            self._collection = None
            self._index = None
            logger.info("Index cleared.")
        except Exception as e:
            logger.warning("Failed to clear index: %s", e)

        # Remove metadata file
        metadata_path = self.db_path / METADATA_FILE
        if metadata_path.exists():
            metadata_path.unlink()


def create_vector_store(
    settings: Settings,
    with_rerank: bool = True,
) -> VectorStore:
    """Factory function to create VectorStore from settings.

    Args:
        settings: Application settings
        with_rerank: Whether to enable reranking

    Returns:
        Configured VectorStore instance
    """
    from cangjie_mcp.indexer.embeddings import get_embedding_provider
    from cangjie_mcp.indexer.reranker import get_reranker_provider

    embedding_provider = get_embedding_provider(settings)
    reranker = get_reranker_provider(settings) if with_rerank and settings.rerank_type != "none" else None

    return VectorStore(
        db_path=settings.chroma_db_dir,
        embedding_provider=embedding_provider,
        reranker=reranker,
    )
