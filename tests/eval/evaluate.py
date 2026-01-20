"""RAG evaluation runner for Cangjie MCP.

Usage:
    uv run python -m tests.eval.evaluate [--top-k 10] [--output results.json]

Requires an existing index to be built first:
    uv run cangjie-mcp index --version latest --lang zh
"""

import argparse
import json
import sys
from pathlib import Path
from typing import Any

from rich.console import Console
from rich.table import Table

from cangjie_mcp.config import Settings, reset_settings, set_settings
from cangjie_mcp.indexer.embeddings import get_embedding_provider, reset_embedding_provider
from cangjie_mcp.indexer.reranker import get_reranker_provider, reset_reranker_provider
from cangjie_mcp.indexer.store import VectorStore
from tests.eval.metrics import EvaluationMetrics, RetrievalResult

console = Console()

# Path to test queries
QUERIES_FILE = Path(__file__).parent / "test_queries.json"


def load_test_queries() -> list[dict[str, Any]]:
    """Load test queries from JSON file."""
    with QUERIES_FILE.open(encoding="utf-8") as f:
        data = json.load(f)
    return data["queries"]


def run_evaluation(
    settings: Settings,
    top_k: int = 10,
    queries: list[dict[str, Any]] | None = None,
) -> EvaluationMetrics:
    """Run RAG evaluation with the given settings.

    Args:
        settings: Application settings
        top_k: Number of results to retrieve per query
        queries: Optional list of query dicts, defaults to loading from file

    Returns:
        EvaluationMetrics with all results
    """
    if queries is None:
        queries = load_test_queries()

    # Check if index exists
    if not settings.chroma_db_dir.exists():
        console.print(f"[red]Index not found at {settings.chroma_db_dir}[/red]")
        console.print("[yellow]Please build the index first:[/yellow]")
        console.print("  uv run cangjie-mcp index --version latest --lang zh")
        sys.exit(1)

    # Initialize providers
    embedding_provider = get_embedding_provider(settings)
    reranker_provider = get_reranker_provider(settings)

    # Create vector store with reranker
    store = VectorStore(
        db_path=settings.chroma_db_dir,
        embedding_provider=embedding_provider,
        reranker=reranker_provider,
    )

    console.print(f"[blue]Running evaluation with {len(queries)} queries...[/blue]")
    console.print(f"  Index: {settings.chroma_db_dir}")
    console.print(f"  Top-K: {top_k}")
    console.print(f"  Embedding: {embedding_provider.get_model_name()}")
    console.print(f"  Reranker: {reranker_provider.get_model_name()}")
    console.print()

    results: list[RetrievalResult] = []

    for query_data in queries:
        query_id = query_data["id"]
        query = query_data["query"]
        expected_files = query_data["expected_files"]
        expected_keywords = query_data.get("expected_keywords", [])

        # Run search
        search_results = store.search(
            query=query,
            top_k=top_k,
            use_rerank=True,
        )

        # Extract file paths and texts
        retrieved_files = [r.metadata.file_path for r in search_results]
        retrieved_texts = [r.text for r in search_results]
        scores = [r.score for r in search_results]

        result = RetrievalResult(
            query_id=query_id,
            query=query,
            expected_files=expected_files,
            retrieved_files=retrieved_files,
            retrieved_texts=retrieved_texts,
            expected_keywords=expected_keywords,
            scores=scores,
        )
        results.append(result)

        # Progress indicator
        status = "[green]OK[/green]" if result.hit else "[red]MISS[/red]"
        console.print(f"  {status} [{query_id}] {query[:40]}...")

    return EvaluationMetrics(results=results)


def print_detailed_results(metrics: EvaluationMetrics) -> None:
    """Print detailed results table."""
    table = Table(title="Query Results")
    table.add_column("ID", style="cyan")
    table.add_column("Query", style="white")
    table.add_column("Hit", style="green")
    table.add_column("RR", style="yellow")
    table.add_column("KW%", style="blue")

    for r in metrics.results:
        hit_str = "Y" if r.hit else "N"
        hit_style = "green" if r.hit else "red"
        table.add_row(
            r.query_id,
            r.query[:30] + "..." if len(r.query) > 30 else r.query,
            f"[{hit_style}]{hit_str}[/{hit_style}]",
            f"{r.reciprocal_rank:.2f}",
            f"{r.keyword_recall:.0%}",
        )

    console.print(table)


def save_results(metrics: EvaluationMetrics, output_path: Path) -> None:
    """Save evaluation results to JSON file."""
    data = {
        "summary": {
            "total_queries": metrics.total_queries,
            "hit_rate": metrics.hit_rate,
            "mrr": metrics.mrr,
            "keyword_recall": metrics.mean_keyword_recall,
            "hit_rate_at_k": metrics.hit_rate_at_k,
            "precision_at_k": metrics.mean_precision_at_k,
            "recall_at_k": metrics.mean_recall_at_k,
        },
        "queries": [
            {
                "id": r.query_id,
                "query": r.query,
                "expected_files": r.expected_files,
                "retrieved_files": r.retrieved_files[:5],  # Top 5 only
                "hit": r.hit,
                "reciprocal_rank": r.reciprocal_rank,
                "keyword_recall": r.keyword_recall,
            }
            for r in metrics.results
        ],
    }

    with output_path.open("w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)

    console.print(f"\n[green]Results saved to {output_path}[/green]")


def main() -> None:
    """Main entry point for evaluation."""
    parser = argparse.ArgumentParser(description="RAG Evaluation for Cangjie MCP")
    parser.add_argument(
        "--top-k",
        type=int,
        default=10,
        help="Number of results to retrieve per query (default: 10)",
    )
    parser.add_argument(
        "--output",
        type=Path,
        help="Output file for results JSON",
    )
    parser.add_argument(
        "--version",
        type=str,
        default="latest",
        help="Documentation version to evaluate (default: latest)",
    )
    parser.add_argument(
        "--lang",
        type=str,
        default="zh",
        choices=["zh", "en"],
        help="Documentation language (default: zh)",
    )
    parser.add_argument(
        "--detailed",
        action="store_true",
        help="Show detailed results table",
    )
    parser.add_argument(
        "--embedding",
        type=str,
        choices=["local", "openai"],
        help="Override embedding type",
    )
    parser.add_argument(
        "--rerank",
        type=str,
        choices=["none", "local", "openai"],
        help="Override reranker type",
    )

    args = parser.parse_args()

    # Reset and create settings
    reset_settings()
    reset_embedding_provider()
    reset_reranker_provider()

    # Build settings with optional overrides
    settings_kwargs: dict[str, Any] = {
        "docs_version": args.version,
        "docs_lang": args.lang,
    }
    if args.embedding:
        settings_kwargs["embedding_type"] = args.embedding
    if args.rerank:
        settings_kwargs["rerank_type"] = args.rerank

    settings = Settings(**settings_kwargs)
    set_settings(settings)

    # Run evaluation
    metrics = run_evaluation(settings, top_k=args.top_k)

    # Print report
    if args.detailed:
        print_detailed_results(metrics)

    metrics.print_report()

    # Save results if output specified
    if args.output:
        save_results(metrics, args.output)


if __name__ == "__main__":
    main()
