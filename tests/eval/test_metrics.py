"""Unit tests for RAG evaluation metrics."""

import pytest

from tests.eval.metrics import EvaluationMetrics, RetrievalResult


class TestRetrievalResult:
    """Tests for RetrievalResult class."""

    def test_hit_when_expected_found(self) -> None:
        """Test hit returns True when expected file is found."""
        result = RetrievalResult(
            query_id="test",
            query="test query",
            expected_files=["class.md"],
            retrieved_files=["other.md", "class.md", "another.md"],
            retrieved_texts=["text1", "text2", "text3"],
            expected_keywords=[],
        )
        assert result.hit is True

    def test_hit_when_partial_match(self) -> None:
        """Test hit returns True for partial path match."""
        result = RetrievalResult(
            query_id="test",
            query="test query",
            expected_files=["class_and_interface/class.md"],
            retrieved_files=["path/to/class_and_interface/class.md"],
            retrieved_texts=["text"],
            expected_keywords=[],
        )
        assert result.hit is True

    def test_hit_when_not_found(self) -> None:
        """Test hit returns False when expected file not found."""
        result = RetrievalResult(
            query_id="test",
            query="test query",
            expected_files=["class.md"],
            retrieved_files=["other.md", "another.md"],
            retrieved_texts=["text1", "text2"],
            expected_keywords=[],
        )
        assert result.hit is False

    def test_hit_at_k(self) -> None:
        """Test hit_at_k for various k values."""
        result = RetrievalResult(
            query_id="test",
            query="test query",
            expected_files=["target.md"],
            retrieved_files=["a.md", "b.md", "c.md", "target.md", "e.md"],
            retrieved_texts=[""] * 5,
            expected_keywords=[],
        )
        assert result.hit_at_k[1] is False
        assert result.hit_at_k[3] is False
        assert result.hit_at_k[5] is True
        assert result.hit_at_k[10] is True

    def test_reciprocal_rank_first(self) -> None:
        """Test RR when relevant doc is first."""
        result = RetrievalResult(
            query_id="test",
            query="test query",
            expected_files=["target.md"],
            retrieved_files=["target.md", "other.md"],
            retrieved_texts=["", ""],
            expected_keywords=[],
        )
        assert result.reciprocal_rank == 1.0

    def test_reciprocal_rank_third(self) -> None:
        """Test RR when relevant doc is third."""
        result = RetrievalResult(
            query_id="test",
            query="test query",
            expected_files=["target.md"],
            retrieved_files=["a.md", "b.md", "target.md"],
            retrieved_texts=["", "", ""],
            expected_keywords=[],
        )
        assert result.reciprocal_rank == pytest.approx(1.0 / 3)

    def test_reciprocal_rank_not_found(self) -> None:
        """Test RR when relevant doc not found."""
        result = RetrievalResult(
            query_id="test",
            query="test query",
            expected_files=["target.md"],
            retrieved_files=["a.md", "b.md"],
            retrieved_texts=["", ""],
            expected_keywords=[],
        )
        assert result.reciprocal_rank == 0.0

    def test_precision_at_k(self) -> None:
        """Test precision at various k values."""
        result = RetrievalResult(
            query_id="test",
            query="test query",
            expected_files=["target.md"],
            retrieved_files=["target.md", "other.md", "target.md"],
            retrieved_texts=["", "", ""],
            expected_keywords=[],
        )
        assert result.precision_at_k[1] == 1.0  # 1/1
        assert result.precision_at_k[3] == pytest.approx(2.0 / 3)  # 2/3

    def test_recall_at_k(self) -> None:
        """Test recall at various k values."""
        result = RetrievalResult(
            query_id="test",
            query="test query",
            expected_files=["a.md", "b.md"],
            retrieved_files=["a.md", "c.md", "b.md", "d.md"],
            retrieved_texts=["", "", "", ""],
            expected_keywords=[],
        )
        assert result.recall_at_k[1] == 0.5  # Found a.md, missing b.md
        assert result.recall_at_k[3] == 1.0  # Found both

    def test_keyword_recall_all_found(self) -> None:
        """Test keyword recall when all keywords found."""
        result = RetrievalResult(
            query_id="test",
            query="test query",
            expected_files=[],
            retrieved_files=[],
            retrieved_texts=["This is a class definition", "with constructor"],
            expected_keywords=["class", "constructor"],
        )
        assert result.keyword_recall == 1.0

    def test_keyword_recall_partial(self) -> None:
        """Test keyword recall with partial match."""
        result = RetrievalResult(
            query_id="test",
            query="test query",
            expected_files=[],
            retrieved_files=[],
            retrieved_texts=["This is a class definition"],
            expected_keywords=["class", "interface", "enum"],
        )
        assert result.keyword_recall == pytest.approx(1.0 / 3)

    def test_keyword_recall_case_insensitive(self) -> None:
        """Test keyword recall is case insensitive."""
        result = RetrievalResult(
            query_id="test",
            query="test query",
            expected_files=[],
            retrieved_files=[],
            retrieved_texts=["CLASS definition"],
            expected_keywords=["class"],
        )
        assert result.keyword_recall == 1.0


class TestEvaluationMetrics:
    """Tests for EvaluationMetrics class."""

    def test_hit_rate(self) -> None:
        """Test overall hit rate calculation."""
        results = [
            RetrievalResult(
                query_id="1",
                query="q1",
                expected_files=["a.md"],
                retrieved_files=["a.md"],
                retrieved_texts=[""],
                expected_keywords=[],
            ),
            RetrievalResult(
                query_id="2",
                query="q2",
                expected_files=["b.md"],
                retrieved_files=["c.md"],
                retrieved_texts=[""],
                expected_keywords=[],
            ),
        ]
        metrics = EvaluationMetrics(results=results)
        assert metrics.hit_rate == 0.5

    def test_mrr(self) -> None:
        """Test MRR calculation."""
        results = [
            RetrievalResult(
                query_id="1",
                query="q1",
                expected_files=["a.md"],
                retrieved_files=["a.md", "b.md"],  # RR = 1
                retrieved_texts=["", ""],
                expected_keywords=[],
            ),
            RetrievalResult(
                query_id="2",
                query="q2",
                expected_files=["b.md"],
                retrieved_files=["a.md", "b.md"],  # RR = 0.5
                retrieved_texts=["", ""],
                expected_keywords=[],
            ),
        ]
        metrics = EvaluationMetrics(results=results)
        assert metrics.mrr == pytest.approx(0.75)  # (1 + 0.5) / 2

    def test_get_failed_queries(self) -> None:
        """Test getting failed queries."""
        results = [
            RetrievalResult(
                query_id="1",
                query="q1",
                expected_files=["a.md"],
                retrieved_files=["a.md"],
                retrieved_texts=[""],
                expected_keywords=[],
            ),
            RetrievalResult(
                query_id="2",
                query="q2",
                expected_files=["b.md"],
                retrieved_files=["c.md"],
                retrieved_texts=[""],
                expected_keywords=[],
            ),
        ]
        metrics = EvaluationMetrics(results=results)
        failed = metrics.get_failed_queries()
        assert len(failed) == 1
        assert failed[0].query_id == "2"

    def test_summary(self) -> None:
        """Test summary contains all expected keys."""
        metrics = EvaluationMetrics(results=[])
        summary = metrics.summary()

        assert "total_queries" in summary
        assert "hit_rate" in summary
        assert "mrr" in summary
        assert "hit_rate@k" in summary
        assert "precision@k" in summary
        assert "recall@k" in summary
        assert "keyword_recall" in summary
