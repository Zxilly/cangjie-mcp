"""Semantic document chunking using LlamaIndex."""

from llama_index.core import Document
from llama_index.core.node_parser import (
    NodeParser,
    SemanticSplitterNodeParser,
    SentenceSplitter,
)
from llama_index.core.schema import BaseNode
from rich.console import Console

from cangjie_mcp.indexer.embeddings import EmbeddingProvider

console = Console()


class DocumentChunker:
    """Chunks documents using semantic splitting."""

    def __init__(
        self,
        embedding_provider: EmbeddingProvider,
        buffer_size: int = 1,
        breakpoint_percentile_threshold: int = 95,
    ) -> None:
        """Initialize the document chunker.

        Args:
            embedding_provider: Embedding provider for semantic splitting
            buffer_size: Number of sentences to group for comparison
            breakpoint_percentile_threshold: Percentile threshold for splitting
        """
        self.embedding_provider = embedding_provider
        self.buffer_size = buffer_size
        self.breakpoint_percentile_threshold = breakpoint_percentile_threshold
        self._splitter: SemanticSplitterNodeParser | None = None
        self._fallback_splitter: SentenceSplitter | None = None

    def _get_semantic_splitter(self) -> SemanticSplitterNodeParser:
        """Get or create the semantic splitter."""
        if self._splitter is None:
            embed_model = self.embedding_provider.get_embedding_model()
            self._splitter = SemanticSplitterNodeParser(
                buffer_size=self.buffer_size,
                breakpoint_percentile_threshold=self.breakpoint_percentile_threshold,
                embed_model=embed_model,
            )
        return self._splitter

    def _get_fallback_splitter(self) -> SentenceSplitter:
        """Get or create the fallback sentence splitter."""
        if self._fallback_splitter is None:
            self._fallback_splitter = SentenceSplitter(
                chunk_size=1024,
                chunk_overlap=200,
            )
        return self._fallback_splitter

    def chunk_documents(
        self,
        documents: list[Document],
        use_semantic: bool = True,
    ) -> list[BaseNode]:
        """Chunk documents into text nodes.

        Args:
            documents: List of documents to chunk
            use_semantic: Whether to use semantic splitting (slower but better)

        Returns:
            List of text nodes
        """
        if not documents:
            return []

        console.print(f"[blue]Chunking {len(documents)} documents...[/blue]")

        splitter: NodeParser
        if use_semantic:
            try:
                splitter = self._get_semantic_splitter()
                nodes = splitter.get_nodes_from_documents(documents, show_progress=True)
            except Exception as e:
                console.print(
                    f"[yellow]Semantic splitting failed: {e}. "
                    "Falling back to sentence splitting.[/yellow]"
                )
                splitter = self._get_fallback_splitter()
                nodes = splitter.get_nodes_from_documents(documents, show_progress=True)
        else:
            splitter = self._get_fallback_splitter()
            nodes = splitter.get_nodes_from_documents(documents, show_progress=True)

        console.print(f"[green]Created {len(nodes)} chunks.[/green]")
        return nodes

    def chunk_single_document(
        self,
        document: Document,
        use_semantic: bool = True,
    ) -> list[BaseNode]:
        """Chunk a single document.

        Args:
            document: Document to chunk
            use_semantic: Whether to use semantic splitting

        Returns:
            List of text nodes
        """
        return self.chunk_documents([document], use_semantic=use_semantic)


def create_chunker(embedding_provider: EmbeddingProvider) -> DocumentChunker:
    """Factory function to create a document chunker.

    Args:
        embedding_provider: Embedding provider for semantic splitting

    Returns:
        Configured DocumentChunker instance
    """
    return DocumentChunker(embedding_provider=embedding_provider)
