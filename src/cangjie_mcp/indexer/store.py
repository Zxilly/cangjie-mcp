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
from rich.console import Console

from cangjie_mcp.indexer.embeddings import EmbeddingProvider

if TYPE_CHECKING:
    from chromadb.api import ClientAPI
    from chromadb.api.models.Collection import Collection

console = Console()

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
    ) -> None:
        """Initialize vector store.

        Args:
            db_path: Path to ChromaDB storage directory
            embedding_provider: Embedding provider for vectorization
            collection_name: Name of the ChromaDB collection
        """
        self.db_path = db_path
        self.embedding_provider = embedding_provider
        self.collection_name = collection_name
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
            return IndexMetadata.model_validate_json(
                metadata_path.read_text(encoding="utf-8")
            )
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
        console.print(f"[blue]Indexing {len(nodes)} nodes into ChromaDB...[/blue]")

        storage_context = self._reset_collection()
        embed_model = self.embedding_provider.get_embedding_model()

        self._index = VectorStoreIndex(
            nodes=nodes,
            storage_context=storage_context,
            embed_model=embed_model,
            show_progress=True,
        )

        console.print("[green]Indexing complete.[/green]")
        return self._index

    def index_documents(self, documents: list[Document]) -> VectorStoreIndex:
        """Index documents directly (uses default chunking).

        Args:
            documents: List of documents to index

        Returns:
            VectorStoreIndex for querying
        """
        console.print(f"[blue]Indexing {len(documents)} documents into ChromaDB...[/blue]")

        storage_context = self._reset_collection()
        embed_model = self.embedding_provider.get_embedding_model()

        self._index = VectorStoreIndex.from_documents(
            documents=documents,
            storage_context=storage_context,
            embed_model=embed_model,
            show_progress=True,
        )

        console.print("[green]Indexing complete.[/green]")
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
        self._index = VectorStoreIndex.from_vector_store(
            vector_store=vector_store,
            embed_model=embed_model,
        )
        return self._index

    def search(
        self,
        query: str,
        top_k: int = 5,
        category: str | None = None,
    ) -> list[SearchResult]:
        """Search for documents matching query.

        Args:
            query: Search query
            top_k: Number of results to return
            category: Optional category filter

        Returns:
            List of search results with text and metadata
        """
        index = self.get_index()
        if index is None:
            return []

        # Build retriever with filters
        filters = None
        if category:
            from llama_index.core.vector_stores import MetadataFilter, MetadataFilters
            filters = MetadataFilters(
                filters=[MetadataFilter(key="category", value=category)]
            )

        retriever = index.as_retriever(
            similarity_top_k=top_k,
            filters=filters,
        )

        nodes = retriever.retrieve(query)

        results: list[SearchResult] = []
        for node in nodes:
            metadata = SearchResultMetadata(
                file_path=str(node.metadata.get("file_path", "")),
                category=str(node.metadata.get("category", "")),
                topic=str(node.metadata.get("topic", "")),
                title=str(node.metadata.get("title", "")),
            )
            results.append(SearchResult(
                text=node.text,
                score=node.score if node.score is not None else 0.0,
                metadata=metadata,
            ))

        return results

    def clear(self) -> None:
        """Clear all indexed data."""
        try:
            self.client.delete_collection(self.collection_name)
            self._collection = None
            self._index = None
            console.print("[green]Index cleared.[/green]")
        except Exception as e:
            console.print(f"[yellow]Warning: Failed to clear index: {e}[/yellow]")

        # Remove metadata file
        metadata_path = self.db_path / METADATA_FILE
        if metadata_path.exists():
            metadata_path.unlink()
