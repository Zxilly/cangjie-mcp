"""Reranker abstraction layer for improving search result relevance.

Uses LlamaIndex's native SentenceTransformerRerank for local reranking,
and custom implementation for OpenAI-compatible rerank APIs (SiliconFlow, etc.).
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from typing import TYPE_CHECKING

from llama_index.core.postprocessor import SentenceTransformerRerank
from rich.console import Console

from cangjie_mcp.utils import SingletonProvider

if TYPE_CHECKING:
    from llama_index.core.schema import NodeWithScore

    from cangjie_mcp.config import Settings

console = Console()


class RerankerProvider(ABC):
    """Abstract base class for reranker providers."""

    @abstractmethod
    def rerank(
        self,
        query: str,
        nodes: list[NodeWithScore],
        top_k: int = 5,
    ) -> list[NodeWithScore]:
        """Rerank nodes based on query relevance.

        Args:
            query: The search query
            nodes: List of nodes to rerank
            top_k: Number of top results to return after reranking

        Returns:
            Reranked list of nodes
        """
        pass

    @abstractmethod
    def get_model_name(self) -> str:
        """Get the model name for identification.

        Returns:
            Model name string
        """
        pass


class NoOpReranker(RerankerProvider):
    """No-op reranker that returns nodes unchanged (for when reranking is disabled)."""

    def rerank(
        self,
        query: str,  # noqa: ARG002
        nodes: list[NodeWithScore],
        top_k: int = 5,
    ) -> list[NodeWithScore]:
        """Return nodes unchanged."""
        return nodes[:top_k]

    def get_model_name(self) -> str:
        """Get the model name."""
        return "none"


class LocalReranker(RerankerProvider):
    """Local reranker using LlamaIndex's SentenceTransformerRerank.

    Uses cross-encoder models from HuggingFace via sentence-transformers.
    """

    DEFAULT_MODEL = "BAAI/bge-reranker-v2-m3"

    def __init__(
        self,
        model_name: str = DEFAULT_MODEL,
        device: str = "cpu",
    ) -> None:
        """Initialize local reranker.

        Args:
            model_name: HuggingFace cross-encoder model name
            device: Device to use (cuda, cpu, mps).
        """
        self.model_name = model_name
        self.device = device
        self._reranker: SentenceTransformerRerank | None = None

    def _get_reranker(self, top_n: int) -> SentenceTransformerRerank:
        """Get or create the SentenceTransformerRerank instance."""
        if self._reranker is None or self._reranker.top_n != top_n:
            console.print(f"[blue]Loading local reranker model: {self.model_name}...[/blue]")
            self._reranker = SentenceTransformerRerank(
                model=self.model_name,
                top_n=top_n,
                device=self.device,
            )
            console.print("[green]Local reranker model loaded.[/green]")
        return self._reranker

    def rerank(
        self,
        query: str,
        nodes: list[NodeWithScore],
        top_k: int = 5,
    ) -> list[NodeWithScore]:
        """Rerank nodes using LlamaIndex's SentenceTransformerRerank."""
        if not nodes:
            return []

        from llama_index.core.schema import QueryBundle

        reranker = self._get_reranker(top_n=top_k)
        query_bundle = QueryBundle(query_str=query)

        # Use LlamaIndex's native postprocessor
        reranked = reranker.postprocess_nodes(nodes, query_bundle)

        return reranked

    def get_model_name(self) -> str:
        """Get the model name."""
        return f"local:{self.model_name}"


class OpenAICompatibleReranker(RerankerProvider):
    """Reranker using OpenAI-compatible rerank API.

    Works with SiliconFlow, Jina, and other providers that implement
    the OpenAI-compatible /rerank endpoint.
    """

    DEFAULT_BASE_URL = "https://api.openai.com/v1"
    DEFAULT_MODEL = "BAAI/bge-reranker-v2-m3"

    def __init__(
        self,
        api_key: str,
        model: str = DEFAULT_MODEL,
        base_url: str = DEFAULT_BASE_URL,
    ) -> None:
        """Initialize OpenAI-compatible reranker.

        Args:
            api_key: API key for the provider
            model: Reranker model name (default: BAAI/bge-reranker-v2-m3)
            base_url: API base URL (e.g., https://api.siliconflow.cn/v1)
        """
        self.api_key = api_key
        self.model = model
        self.base_url = base_url.rstrip("/")

    def rerank(
        self,
        query: str,
        nodes: list[NodeWithScore],
        top_k: int = 5,
    ) -> list[NodeWithScore]:
        """Rerank nodes using OpenAI-compatible rerank API."""
        if not nodes:
            return []

        import httpx

        console.print(f"[blue]Reranking {len(nodes)} results with API ({self.base_url})...[/blue]")

        # Prepare documents for API call
        documents = [node.text for node in nodes]

        # Call rerank API
        response = httpx.post(
            f"{self.base_url}/rerank",
            headers={
                "Authorization": f"Bearer {self.api_key}",
                "Content-Type": "application/json",
            },
            json={
                "model": self.model,
                "query": query,
                "documents": documents,
                "top_n": top_k,
                "return_documents": False,
            },
            timeout=30.0,
        )
        response.raise_for_status()

        result = response.json()

        # Reconstruct nodes with new scores
        reranked: list[NodeWithScore] = []
        for item in result.get("results", []):
            idx: int = item["index"]
            rerank_score: float = item["relevance_score"]
            node: NodeWithScore = nodes[idx]
            node.node.metadata["original_score"] = str(node.score if node.score else 0.0)
            node.score = rerank_score
            reranked.append(node)

        console.print("[green]Reranking complete.[/green]")
        return reranked

    def get_model_name(self) -> str:
        """Get the model name."""
        return f"openai:{self.model}"


def create_reranker_provider(
    rerank_type: str = "none",
    local_model: str = LocalReranker.DEFAULT_MODEL,
    api_key: str | None = None,
    api_model: str | None = None,
    api_base_url: str | None = None,
) -> RerankerProvider:
    """Factory function to create reranker provider.

    Args:
        rerank_type: Type of reranker (none, local, openai)
        local_model: Model name for local reranking
        api_key: API key for OpenAI-compatible reranker
        api_model: Model name for reranking
        api_base_url: Base URL for OpenAI-compatible API

    Returns:
        Configured reranker provider

    Raises:
        ValueError: If OpenAI reranker is selected but API key is not set
    """
    match rerank_type:
        case "none":
            return NoOpReranker()
        case "local":
            return LocalReranker(model_name=local_model)
        case "openai":
            if not api_key:
                raise ValueError("OpenAI API key is required for OpenAI-compatible reranker")
            return OpenAICompatibleReranker(
                api_key=api_key,
                model=api_model or OpenAICompatibleReranker.DEFAULT_MODEL,
                base_url=api_base_url or OpenAICompatibleReranker.DEFAULT_BASE_URL,
            )
        case _:
            raise ValueError(f"Unknown rerank type: {rerank_type}")


def _create_reranker_from_settings(settings: Settings) -> RerankerProvider:
    """Create reranker provider from settings.

    Args:
        settings: Application settings

    Returns:
        Configured reranker provider
    """
    return create_reranker_provider(
        rerank_type=settings.rerank_type,
        local_model=settings.rerank_model,
        api_key=settings.openai_api_key,
        api_model=settings.rerank_model,
        api_base_url=settings.openai_base_url,
    )


# Global reranker provider singleton
_reranker_provider = SingletonProvider[RerankerProvider](_create_reranker_from_settings)


def get_reranker_provider(settings: Settings | None = None) -> RerankerProvider:
    """Get or create the global reranker provider.

    Args:
        settings: Optional settings to use for creation

    Returns:
        The reranker provider instance
    """
    return _reranker_provider.get(settings)


def reset_reranker_provider() -> None:
    """Reset the global reranker provider (useful for testing)."""
    _reranker_provider.reset()
