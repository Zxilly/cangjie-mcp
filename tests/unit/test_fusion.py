"""Tests for RRF (Reciprocal Rank Fusion)."""

from cangjie_mcp.indexer.fusion import reciprocal_rank_fusion
from cangjie_mcp.indexer.store import SearchResult, SearchResultMetadata


def _make_result(text: str, score: float = 0.5, file_path: str = "", category: str = "") -> SearchResult:
    return SearchResult(
        text=text,
        score=score,
        metadata=SearchResultMetadata(
            file_path=file_path or f"{text[:10]}.md",
            category=category,
            topic="test",
            title="Test",
        ),
    )


class TestReciprocalRankFusion:
    """Tests for reciprocal_rank_fusion function."""

    def test_single_list(self) -> None:
        results = [_make_result("doc1", 0.9), _make_result("doc2", 0.8)]
        fused = reciprocal_rank_fusion([results], k=60, top_k=5)
        assert len(fused) == 2
        # Order should be preserved
        assert fused[0].text == "doc1"
        assert fused[1].text == "doc2"

    def test_two_lists_disjoint(self) -> None:
        list1 = [_make_result("doc_a"), _make_result("doc_b")]
        list2 = [_make_result("doc_c"), _make_result("doc_d")]
        fused = reciprocal_rank_fusion([list1, list2], k=60, top_k=4)
        assert len(fused) == 4
        texts = {r.text for r in fused}
        assert texts == {"doc_a", "doc_b", "doc_c", "doc_d"}

    def test_overlapping_results_boost(self) -> None:
        """Results appearing in both lists should rank higher."""
        common = _make_result("common_doc", file_path="common.md")
        list1 = [common, _make_result("only_in_1")]
        list2 = [_make_result("common_doc", file_path="common.md"), _make_result("only_in_2")]
        fused = reciprocal_rank_fusion([list1, list2], k=60, top_k=5)
        # The common result should be first due to accumulated score
        assert fused[0].text == "common_doc"

    def test_top_k_limit(self) -> None:
        results = [_make_result(f"doc{i}", file_path=f"doc{i}.md") for i in range(10)]
        fused = reciprocal_rank_fusion([results], k=60, top_k=3)
        assert len(fused) == 3

    def test_empty_input(self) -> None:
        assert reciprocal_rank_fusion([], k=60, top_k=5) == []

    def test_empty_lists(self) -> None:
        assert reciprocal_rank_fusion([[], []], k=60, top_k=5) == []

    def test_scores_are_positive(self) -> None:
        results = [_make_result("doc1"), _make_result("doc2")]
        fused = reciprocal_rank_fusion([results], k=60, top_k=5)
        for r in fused:
            assert r.score > 0

    def test_scores_descending(self) -> None:
        list1 = [_make_result(f"d{i}", file_path=f"d{i}.md") for i in range(5)]
        list2 = [_make_result(f"d{i}", file_path=f"d{i}.md") for i in range(3, 8)]
        fused = reciprocal_rank_fusion([list1, list2], k=60, top_k=10)
        scores = [r.score for r in fused]
        assert scores == sorted(scores, reverse=True)

    def test_metadata_preserved(self) -> None:
        result = _make_result("test doc", file_path="test.md", category="cat1")
        fused = reciprocal_rank_fusion([[result]], k=60, top_k=5)
        assert fused[0].metadata.file_path == "test.md"
        assert fused[0].metadata.category == "cat1"
