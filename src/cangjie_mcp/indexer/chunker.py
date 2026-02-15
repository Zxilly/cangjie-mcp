"""Semantic document chunking using LlamaIndex with markdown and code awareness."""

import re
from dataclasses import dataclass

from llama_index.core import Document
from llama_index.core.node_parser import (
    MarkdownNodeParser,
    NodeParser,
    SemanticSplitterNodeParser,
    SentenceSplitter,
)
from llama_index.core.schema import BaseNode, TextNode

from cangjie_mcp.defaults import DEFAULT_CHUNK_MAX_SIZE
from cangjie_mcp.indexer.embeddings import EmbeddingProvider
from cangjie_mcp.utils import logger

# Regex pattern to match fenced code blocks (```language\ncode\n```)
CODE_BLOCK_PATTERN = re.compile(r"```[\w]*\n.*?```", re.DOTALL)


@dataclass
class CodeAwareSegment:
    """A segment of markdown content with code awareness."""

    text: str
    has_code: bool  # Whether this segment contains code blocks
    code_count: int  # Number of code blocks in this segment


class DocumentChunker:
    """Chunks documents using markdown-aware and semantic splitting with size limits.

    The chunking strategy is:
    1. First split by markdown sections (H2/H3 headings) for large documents
    2. Apply semantic splitting within sections for better context preservation
    3. Enforce max chunk size to prevent exceeding embedding model token limits
    """

    def __init__(
        self,
        embedding_provider: EmbeddingProvider,
        buffer_size: int = 1,
        breakpoint_percentile_threshold: int = 95,
        max_chunk_size: int = DEFAULT_CHUNK_MAX_SIZE,
    ) -> None:
        """Initialize the document chunker.

        Args:
            embedding_provider: Embedding provider for semantic splitting
            buffer_size: Number of sentences to group for comparison
            breakpoint_percentile_threshold: Percentile threshold for splitting
            max_chunk_size: Maximum chunk size in characters to prevent exceeding
                           embedding model token limits (default: 6000)
        """
        self.embedding_provider = embedding_provider
        self.buffer_size = buffer_size
        self.breakpoint_percentile_threshold = breakpoint_percentile_threshold
        self.max_chunk_size = max_chunk_size
        self._markdown_splitter: MarkdownNodeParser | None = None
        self._splitter: SemanticSplitterNodeParser | None = None
        self._fallback_splitter: SentenceSplitter | None = None
        self._size_limiter: SentenceSplitter | None = None

    def _get_markdown_splitter(self) -> MarkdownNodeParser:
        """Get or create the markdown splitter for section-based splitting."""
        if self._markdown_splitter is None:
            self._markdown_splitter = MarkdownNodeParser()
        return self._markdown_splitter

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

    def _get_size_limiter(self) -> SentenceSplitter:
        """Get or create the size limiter for oversized chunks."""
        if self._size_limiter is None:
            self._size_limiter = SentenceSplitter(
                chunk_size=self.max_chunk_size,
                chunk_overlap=200,
            )
        return self._size_limiter

    def _split_text_preserving_code(self, text: str) -> list[CodeAwareSegment]:
        """Split text into segments while preserving code blocks.

        This ensures code blocks are never split in the middle.
        Each segment contains either:
        - Pure text (can be further split)
        - Text followed by a code block (kept together as context + example)

        Args:
            text: Markdown text to split

        Returns:
            List of CodeAwareSegment objects
        """
        segments: list[CodeAwareSegment] = []
        code_blocks = list(CODE_BLOCK_PATTERN.finditer(text))

        if not code_blocks:
            # No code blocks, return as single text segment
            return [CodeAwareSegment(text=text, has_code=False, code_count=0)]

        last_end = 0
        for match in code_blocks:
            start, end = match.start(), match.end()

            # Get text before this code block (context)
            if start > last_end:
                preceding_text = text[last_end:start]
                # Find the last paragraph/heading as context for the code
                # Split at double newlines to find paragraph boundaries
                paragraphs = preceding_text.rsplit("\n\n", 1)
                if len(paragraphs) == 2 and paragraphs[0].strip():
                    # Add earlier text as separate segment
                    segments.append(
                        CodeAwareSegment(
                            text=paragraphs[0].strip(),
                            has_code=False,
                            code_count=0,
                        )
                    )
                    # Context paragraph + code block together
                    context_and_code = paragraphs[1] + text[start:end]
                    segments.append(
                        CodeAwareSegment(
                            text=context_and_code.strip(),
                            has_code=True,
                            code_count=1,
                        )
                    )
                else:
                    # All preceding text + code block together
                    combined = preceding_text + text[start:end]
                    segments.append(
                        CodeAwareSegment(
                            text=combined.strip(),
                            has_code=True,
                            code_count=1,
                        )
                    )
            else:
                # Code block at the beginning or consecutive code blocks
                segments.append(
                    CodeAwareSegment(
                        text=text[start:end].strip(),
                        has_code=True,
                        code_count=1,
                    )
                )

            last_end = end

        # Add any remaining text after the last code block
        if last_end < len(text):
            remaining = text[last_end:].strip()
            if remaining:
                segments.append(
                    CodeAwareSegment(
                        text=remaining,
                        has_code=False,
                        code_count=0,
                    )
                )

        return segments

    def _enforce_size_limit(self, nodes: list[BaseNode]) -> list[BaseNode]:
        """Split any nodes that exceed the max chunk size, preserving code blocks.

        Code blocks are never split - they are kept intact even if oversized.
        Only pure text segments are split when exceeding the limit.

        Args:
            nodes: List of nodes to check and potentially split

        Returns:
            List of nodes with size limits enforced
        """
        result: list[BaseNode] = []
        split_count = 0
        preserved_code_count = 0

        for node in nodes:
            text = node.get_content()
            if len(text) <= self.max_chunk_size:
                result.append(node)
                continue

            # Use code-aware splitting
            segments = self._split_text_preserving_code(text)

            for segment in segments:
                if segment.has_code:
                    # Keep code segments intact, even if oversized
                    if len(segment.text) > self.max_chunk_size:
                        preserved_code_count += 1
                    sub_node = TextNode(
                        text=segment.text,
                        metadata={**node.metadata, "has_code": True},
                    )
                    result.append(sub_node)
                elif len(segment.text) <= self.max_chunk_size:
                    # Text segment within limit
                    sub_node = TextNode(
                        text=segment.text,
                        metadata={**node.metadata},
                    )
                    result.append(sub_node)
                else:
                    # Split oversized text-only segment
                    split_count += 1
                    size_limiter = self._get_size_limiter()
                    doc = Document(text=segment.text, metadata=node.metadata)
                    split_nodes = size_limiter.get_nodes_from_documents([doc])
                    for split_node in split_nodes:
                        if isinstance(split_node, TextNode):
                            split_node.metadata = {**node.metadata}
                    result.extend(split_nodes)

        if split_count > 0:
            logger.warning("Split %d oversized text segments (>%d chars).", split_count, self.max_chunk_size)
        if preserved_code_count > 0:
            logger.info("Preserved %d oversized code blocks intact.", preserved_code_count)

        return result

    def _split_large_documents(self, documents: list[Document]) -> list[Document]:
        """Pre-split large documents by markdown sections.

        For documents larger than max_chunk_size, use MarkdownNodeParser
        to split by headings first, preserving section context.

        Args:
            documents: List of documents

        Returns:
            List of documents, with large ones split into sections
        """
        result: list[Document] = []
        split_count = 0

        for doc in documents:
            text = doc.get_content()
            if len(text) <= self.max_chunk_size:
                result.append(doc)
            else:
                # Split by markdown sections
                split_count += 1
                md_splitter = self._get_markdown_splitter()
                section_nodes = md_splitter.get_nodes_from_documents([doc])

                # Convert nodes back to documents for further processing
                for node in section_nodes:
                    section_doc = Document(
                        text=node.get_content(),
                        metadata={**doc.metadata},
                        doc_id=f"{doc.doc_id}#{node.id_}" if doc.doc_id else None,
                    )
                    result.append(section_doc)

        if split_count > 0:
            logger.info("Pre-split %d large documents by markdown sections into %d sections.", split_count, len(result))

        return result

    def chunk_documents(
        self,
        documents: list[Document],
        use_semantic: bool = True,
    ) -> list[BaseNode]:
        """Chunk documents into text nodes.

        The chunking strategy is:
        1. Pre-split large documents by markdown sections (H2/H3 headings)
        2. Apply semantic or sentence splitting within sections
        3. Enforce max chunk size to prevent exceeding embedding model limits

        Args:
            documents: List of documents to chunk
            use_semantic: Whether to use semantic splitting (slower but better)

        Returns:
            List of text nodes
        """
        if not documents:
            return []

        logger.info("Chunking %d documents...", len(documents))

        # Step 1: Pre-split large documents by markdown sections
        documents = self._split_large_documents(documents)

        # Step 2: Apply semantic or sentence splitting
        splitter: NodeParser
        if use_semantic:
            try:
                splitter = self._get_semantic_splitter()
                nodes = splitter.get_nodes_from_documents(documents, show_progress=True)
            except Exception as e:
                logger.warning("Semantic splitting failed: %s. Falling back to sentence splitting.", e)
                splitter = self._get_fallback_splitter()
                nodes = splitter.get_nodes_from_documents(documents, show_progress=True)
        else:
            splitter = self._get_fallback_splitter()
            nodes = splitter.get_nodes_from_documents(documents, show_progress=True)

        # Step 3: Enforce size limits to prevent exceeding embedding model token limits
        nodes = self._enforce_size_limit(nodes)

        logger.info("Created %d chunks.", len(nodes))
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


def create_chunker(
    embedding_provider: EmbeddingProvider,
    max_chunk_size: int = DEFAULT_CHUNK_MAX_SIZE,
) -> DocumentChunker:
    """Factory function to create a document chunker.

    Args:
        embedding_provider: Embedding provider for semantic splitting
        max_chunk_size: Maximum chunk size in characters

    Returns:
        Configured DocumentChunker instance
    """
    return DocumentChunker(
        embedding_provider=embedding_provider,
        max_chunk_size=max_chunk_size,
    )
