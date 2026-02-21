"""BM25 search store using bm25s library with jieba tokenization."""

from __future__ import annotations

import json
from collections.abc import Generator, Sequence
from pathlib import Path
from typing import TYPE_CHECKING, Any, cast

from cangjie_mcp.utils import logger

if TYPE_CHECKING:
    from llama_index.core.schema import BaseNode

    from cangjie_mcp.indexer.store import SearchResult

# Metadata file for document mapping
_BM25_METADATA_FILE = "bm25_doc_metadata.json"


def _cangjie_splitter(text: str) -> list[str]:
    """Tokenize text using jieba for Chinese/English mixed content.

    Uses jieba.cut_for_search for fine-grained segmentation suitable for
    search scenarios (e.g. "中华人民共和国" → "中华"/"人民"/"共和"/"共和国"/"中华人民共和国").
    """
    import jieba

    result = cast(Generator[str], jieba.cut_for_search(text.lower()))
    return [w for w in result if w.strip()]


class BM25Store:
    """BM25 search storage based on the bm25s library."""

    def __init__(self, index_dir: Path) -> None:
        """Initialize BM25 store.

        Args:
            index_dir: Directory for storing the BM25 index.
        """
        self._index_dir = index_dir
        self._retriever: Any = None
        self._doc_texts: list[str] = []
        self._doc_metadata: list[dict[str, str]] = []

    def is_indexed(self) -> bool:
        """Check if a BM25 index already exists on disk."""
        return (self._index_dir / _BM25_METADATA_FILE).exists()

    def build_from_nodes(self, nodes: Sequence[BaseNode]) -> None:
        """Build a BM25 index from LlamaIndex TextNode list.

        1. Extract text and metadata from each node.
        2. Tokenize using jieba.
        3. Build bm25s index and persist to disk.
        4. Save document metadata as a separate JSON file.
        """
        import bm25s

        if not nodes:
            logger.warning("No nodes provided for BM25 indexing.")
            return

        self._doc_texts = []
        self._doc_metadata = []

        for node in nodes:
            text = node.get_content()
            self._doc_texts.append(text)
            meta = dict(node.metadata) if node.metadata else {}
            self._doc_metadata.append({k: str(v) for k, v in meta.items()})

        logger.info("Tokenizing %d documents for BM25...", len(self._doc_texts))
        corpus_tokens = [_cangjie_splitter(text) for text in self._doc_texts]

        self._retriever = bm25s.BM25()
        self._retriever.index(corpus_tokens)

        # Persist
        self._index_dir.mkdir(parents=True, exist_ok=True)
        self._retriever.save(str(self._index_dir))

        meta_path = self._index_dir / _BM25_METADATA_FILE
        meta_path.write_text(
            json.dumps(
                {"texts": self._doc_texts, "metadata": self._doc_metadata},
                ensure_ascii=False,
            ),
            encoding="utf-8",
        )
        logger.info("BM25 index built and saved to %s", self._index_dir)

    def load(self) -> bool:
        """Load a persisted BM25 index from disk.

        Returns:
            True if successfully loaded, False otherwise.
        """
        import bm25s

        meta_path = self._index_dir / _BM25_METADATA_FILE
        if not meta_path.exists():
            return False

        try:
            self._retriever = bm25s.BM25.load(str(self._index_dir), load_corpus=False)
            data = json.loads(meta_path.read_text(encoding="utf-8"))
            self._doc_texts = data["texts"]
            self._doc_metadata = data["metadata"]
            logger.info("BM25 index loaded from %s (%d docs)", self._index_dir, len(self._doc_texts))
            return True
        except Exception:
            logger.exception("Failed to load BM25 index from %s", self._index_dir)
            return False

    def search(
        self,
        query: str,
        top_k: int = 5,
        category: str | None = None,
    ) -> list[SearchResult]:
        """Search the BM25 index.

        Args:
            query: Search query string.
            top_k: Number of results to return.
            category: Optional category filter.

        Returns:
            List of SearchResult instances.
        """
        from cangjie_mcp.indexer.store import SearchResult, SearchResultMetadata

        if self._retriever is None or not self._doc_texts:
            return []

        query_tokens = _cangjie_splitter(query)
        if not query_tokens:
            return []

        # Retrieve more candidates when filtering by category
        retrieve_k = min(len(self._doc_texts), top_k * 4 if category else top_k)

        import numpy as np

        results_obj, scores_obj = self._retriever.retrieve([query_tokens], k=retrieve_k)
        doc_indices: list[int] = list(np.asarray(results_obj[0]).flatten())
        doc_scores: list[float] = list(np.asarray(scores_obj[0]).flatten())

        results: list[SearchResult] = []
        for idx, score in zip(doc_indices, doc_scores, strict=True):
            if idx < 0 or idx >= len(self._doc_texts):
                continue
            meta = self._doc_metadata[idx]
            if category and meta.get("category", "") != category:
                continue
            results.append(
                SearchResult(
                    text=self._doc_texts[idx],
                    score=float(score),
                    metadata=SearchResultMetadata(
                        file_path=meta.get("file_path", ""),
                        category=meta.get("category", ""),
                        topic=meta.get("topic", ""),
                        title=meta.get("title", ""),
                        has_code=meta.get("has_code", "False").lower() == "true",
                    ),
                )
            )
            if len(results) >= top_k:
                break

        return results

    def clear(self) -> None:
        """Remove the BM25 index from disk."""
        import shutil

        if self._index_dir.exists():
            shutil.rmtree(self._index_dir, ignore_errors=True)
        self._retriever = None
        self._doc_texts = []
        self._doc_metadata = []
