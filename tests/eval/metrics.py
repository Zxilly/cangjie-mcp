"""RAG evaluation metrics for measuring retrieval quality."""

from dataclasses import dataclass, field


def normalize_path(path: str) -> str:
    """Normalize path separators for cross-platform comparison."""
    return path.replace("\\", "/").lower()


@dataclass
class RetrievalResult:
    """Result of a single retrieval query."""

    query_id: str
    query: str
    expected_files: list[str]
    retrieved_files: list[str]
    retrieved_texts: list[str]
    expected_keywords: list[str]
    scores: list[float] = field(default_factory=list)

    def _is_match(self, expected: str, retrieved: str) -> bool:
        """Check if expected file matches retrieved file (normalized)."""
        exp_norm = normalize_path(expected)
        ret_norm = normalize_path(retrieved)
        return exp_norm in ret_norm

    @property
    def hit(self) -> bool:
        """Check if any expected file was retrieved."""
        return any(any(self._is_match(exp, ret) for exp in self.expected_files) for ret in self.retrieved_files)

    @property
    def hit_at_k(self) -> dict[int, bool]:
        """Check hit at various k values."""
        result = {}
        for k in [1, 3, 5, 10]:
            top_k = self.retrieved_files[:k]
            result[k] = any(any(self._is_match(exp, ret) for exp in self.expected_files) for ret in top_k)
        return result

    @property
    def reciprocal_rank(self) -> float:
        """Calculate reciprocal rank (1/rank of first relevant result)."""
        for i, ret_file in enumerate(self.retrieved_files):
            if any(self._is_match(exp, ret_file) for exp in self.expected_files):
                return 1.0 / (i + 1)
        return 0.0

    @property
    def precision_at_k(self) -> dict[int, float]:
        """Calculate precision at various k values."""
        result = {}
        for k in [1, 3, 5, 10]:
            top_k = self.retrieved_files[:k]
            if not top_k:
                result[k] = 0.0
                continue
            relevant_count = sum(1 for ret in top_k if any(self._is_match(exp, ret) for exp in self.expected_files))
            result[k] = relevant_count / len(top_k)
        return result

    @property
    def recall_at_k(self) -> dict[int, float]:
        """Calculate recall at various k values."""
        if not self.expected_files:
            return {1: 0.0, 3: 0.0, 5: 0.0, 10: 0.0}

        result = {}
        for k in [1, 3, 5, 10]:
            top_k = self.retrieved_files[:k]
            found_count = sum(1 for exp in self.expected_files if any(self._is_match(exp, ret) for ret in top_k))
            result[k] = found_count / len(self.expected_files)
        return result

    @property
    def keyword_recall(self) -> float:
        """Calculate what percentage of expected keywords appear in results."""
        if not self.expected_keywords:
            return 1.0

        all_text = " ".join(self.retrieved_texts).lower()
        found = sum(1 for kw in self.expected_keywords if kw.lower() in all_text)
        return found / len(self.expected_keywords)


@dataclass
class EvaluationMetrics:
    """Aggregated evaluation metrics across all queries."""

    results: list[RetrievalResult]

    @property
    def total_queries(self) -> int:
        """Total number of queries evaluated."""
        return len(self.results)

    @property
    def hit_rate(self) -> float:
        """Overall hit rate (any expected doc retrieved)."""
        if not self.results:
            return 0.0
        hits = sum(1 for r in self.results if r.hit)
        return hits / len(self.results)

    @property
    def hit_rate_at_k(self) -> dict[int, float]:
        """Hit rate at various k values."""
        if not self.results:
            return {1: 0.0, 3: 0.0, 5: 0.0, 10: 0.0}

        result = {}
        for k in [1, 3, 5, 10]:
            hits = sum(1 for r in self.results if r.hit_at_k.get(k, False))
            result[k] = hits / len(self.results)
        return result

    @property
    def mrr(self) -> float:
        """Mean Reciprocal Rank."""
        if not self.results:
            return 0.0
        return sum(r.reciprocal_rank for r in self.results) / len(self.results)

    @property
    def mean_precision_at_k(self) -> dict[int, float]:
        """Mean precision at various k values."""
        if not self.results:
            return {1: 0.0, 3: 0.0, 5: 0.0, 10: 0.0}

        result = {}
        for k in [1, 3, 5, 10]:
            result[k] = sum(r.precision_at_k[k] for r in self.results) / len(self.results)
        return result

    @property
    def mean_recall_at_k(self) -> dict[int, float]:
        """Mean recall at various k values."""
        if not self.results:
            return {1: 0.0, 3: 0.0, 5: 0.0, 10: 0.0}

        result = {}
        for k in [1, 3, 5, 10]:
            result[k] = sum(r.recall_at_k[k] for r in self.results) / len(self.results)
        return result

    @property
    def mean_keyword_recall(self) -> float:
        """Mean keyword recall across all queries."""
        if not self.results:
            return 0.0
        return sum(r.keyword_recall for r in self.results) / len(self.results)

    def get_failed_queries(self) -> list[RetrievalResult]:
        """Get queries that failed to retrieve any expected document."""
        return [r for r in self.results if not r.hit]

    def get_low_keyword_recall_queries(self, threshold: float = 0.5) -> list[RetrievalResult]:
        """Get queries with keyword recall below threshold."""
        return [r for r in self.results if r.keyword_recall < threshold]

    def summary(self) -> dict[str, float | dict[int, float]]:
        """Get a summary of all metrics."""
        return {
            "total_queries": self.total_queries,
            "hit_rate": self.hit_rate,
            "hit_rate@k": self.hit_rate_at_k,
            "mrr": self.mrr,
            "precision@k": self.mean_precision_at_k,
            "recall@k": self.mean_recall_at_k,
            "keyword_recall": self.mean_keyword_recall,
        }

    def print_report(self) -> None:
        """Print a formatted evaluation report."""
        print("\n" + "=" * 60)
        print("RAG Evaluation Report")
        print("=" * 60)
        print(f"\nTotal Queries: {self.total_queries}")
        print(f"\nHit Rate: {self.hit_rate:.2%}")
        print(f"MRR: {self.mrr:.4f}")
        print(f"Keyword Recall: {self.mean_keyword_recall:.2%}")

        print("\nHit Rate @ K:")
        for k, rate in self.hit_rate_at_k.items():
            print(f"  @{k}: {rate:.2%}")

        print("\nPrecision @ K:")
        for k, prec in self.mean_precision_at_k.items():
            print(f"  @{k}: {prec:.2%}")

        print("\nRecall @ K:")
        for k, rec in self.mean_recall_at_k.items():
            print(f"  @{k}: {rec:.2%}")

        failed = self.get_failed_queries()
        if failed:
            print(f"\nFailed Queries ({len(failed)}):")
            for r in failed:
                print(f"  - [{r.query_id}] {r.query}")

        low_kw = self.get_low_keyword_recall_queries(0.5)
        if low_kw:
            print(f"\nLow Keyword Recall (<50%) Queries ({len(low_kw)}):")
            for r in low_kw:
                print(f"  - [{r.query_id}] {r.query} ({r.keyword_recall:.0%})")

        print("\n" + "=" * 60)
