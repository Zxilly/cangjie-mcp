"""Reciprocal Rank Fusion (RRF) for merging multiple result lists."""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from cangjie_mcp.indexer.store import SearchResult


def _dedup_key(result: SearchResult) -> str:
    """Generate a deduplication key from file_path and text prefix."""
    return f"{result.metadata.file_path}|{result.text[:200]}"


def reciprocal_rank_fusion(
    result_lists: list[list[SearchResult]],
    k: int = 60,
    top_k: int = 5,
) -> list[SearchResult]:
    """Merge multiple ranked result lists using Reciprocal Rank Fusion.

    Each result gets a score of ``1 / (k + rank)`` for each list it appears in.
    Results that appear in multiple lists accumulate higher scores.

    Args:
        result_lists: List of ranked result lists from different retrievers.
        k: RRF constant (default 60). Higher values reduce the impact of rank position.
        top_k: Number of results to return.

    Returns:
        Fused list of SearchResult sorted by RRF score descending.
    """
    from cangjie_mcp.indexer.store import SearchResult, SearchResultMetadata

    if not result_lists:
        return []

    # Accumulate RRF scores per unique result
    scores: dict[str, float] = {}
    best_result: dict[str, SearchResult] = {}

    for results in result_lists:
        for rank, result in enumerate(results):
            key = _dedup_key(result)
            rrf_score = 1.0 / (k + rank + 1)  # rank is 0-based, formula uses 1-based
            scores[key] = scores.get(key, 0.0) + rrf_score
            # Keep the result with the highest original score for metadata
            if key not in best_result or result.score > best_result[key].score:
                best_result[key] = result

    # Sort by RRF score descending
    sorted_keys = sorted(scores.keys(), key=lambda x: scores[x], reverse=True)

    fused: list[SearchResult] = []
    for key in sorted_keys[:top_k]:
        original = best_result[key]
        fused.append(
            SearchResult(
                text=original.text,
                score=scores[key],
                metadata=SearchResultMetadata(
                    file_path=original.metadata.file_path,
                    category=original.metadata.category,
                    topic=original.metadata.topic,
                    title=original.metadata.title,
                    has_code=original.metadata.has_code,
                ),
            )
        )

    return fused
