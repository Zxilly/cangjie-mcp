"""Embedding model abstraction layer."""

from abc import ABC, abstractmethod

from llama_index.core.embeddings import BaseEmbedding
from llama_index.embeddings.huggingface import HuggingFaceEmbedding
from llama_index.embeddings.openai import OpenAIEmbedding
from llama_index.embeddings.openai_like import OpenAILikeEmbedding
from rich.console import Console

from cangjie_mcp.config import Settings, get_openai_settings

console = Console()


# Known OpenAI models that work with the standard OpenAIEmbedding class
_OPENAI_NATIVE_MODELS = {
    "text-embedding-ada-002",
    "text-embedding-3-small",
    "text-embedding-3-large",
    "davinci",
    "curie",
    "babbage",
    "ada",
}


class EmbeddingProvider(ABC):
    """Abstract base class for embedding providers."""

    @abstractmethod
    def get_embedding_model(self) -> BaseEmbedding:
        """Get the LlamaIndex embedding model.

        Returns:
            A LlamaIndex-compatible embedding model
        """
        pass

    @abstractmethod
    def get_model_name(self) -> str:
        """Get the model name for identification.

        Returns:
            Model name string
        """
        pass


class LocalEmbedding(EmbeddingProvider):
    """Local embedding using HuggingFace models."""

    def __init__(self, model_name: str = "paraphrase-multilingual-MiniLM-L12-v2") -> None:
        """Initialize local embedding provider.

        Args:
            model_name: HuggingFace model name
        """
        self.model_name = model_name
        self._model: HuggingFaceEmbedding | None = None

    def get_embedding_model(self) -> BaseEmbedding:
        """Get the HuggingFace embedding model."""
        if self._model is None:
            console.print(f"[blue]Loading local embedding model: {self.model_name}...[/blue]")
            self._model = HuggingFaceEmbedding(model_name=self.model_name)
            console.print("[green]Local embedding model loaded.[/green]")
        return self._model

    def get_model_name(self) -> str:
        """Get the model name."""
        return f"local:{self.model_name}"


class OpenAIEmbeddingProvider(EmbeddingProvider):
    """OpenAI embedding provider with custom base URL support.

    Supports both native OpenAI models and OpenAI-compatible APIs
    (like SiliconFlow) with custom models.
    """

    def __init__(
        self,
        api_key: str,
        model: str = "text-embedding-3-small",
        base_url: str = "https://api.openai.com/v1",
    ) -> None:
        """Initialize OpenAI embedding provider.

        Args:
            api_key: OpenAI API key
            model: Embedding model name (OpenAI model or custom model for compatible APIs)
            base_url: API base URL (OpenAI or compatible API)
        """
        self.api_key = api_key
        self.model = model
        self.base_url = base_url
        self._embedding_model: BaseEmbedding | None = None

    def get_embedding_model(self) -> BaseEmbedding:
        """Get the embedding model.

        Uses OpenAIEmbedding for native OpenAI models, and
        OpenAILikeEmbedding for custom models on compatible APIs.
        """
        if self._embedding_model is None:
            console.print(f"[blue]Initializing OpenAI embedding model: {self.model}...[/blue]")

            # Use native OpenAI embedding for known models, OpenAILikeEmbedding otherwise
            if self.model in _OPENAI_NATIVE_MODELS:
                self._embedding_model = OpenAIEmbedding(
                    api_key=self.api_key,
                    model=self.model,
                    api_base=self.base_url,
                )
            else:
                # Use OpenAILikeEmbedding for custom models (e.g., SiliconFlow's BAAI/bge-m3)
                self._embedding_model = OpenAILikeEmbedding(
                    api_key=self.api_key,
                    model_name=self.model,
                    api_base=self.base_url,
                )

            console.print("[green]OpenAI embedding model initialized.[/green]")
        return self._embedding_model

    def get_model_name(self) -> str:
        """Get the model name."""
        return f"openai:{self.model}"


def create_embedding_provider(settings: Settings) -> EmbeddingProvider:
    """Factory function to create embedding provider based on settings.

    Args:
        settings: Application settings

    Returns:
        Configured embedding provider

    Raises:
        ValueError: If OpenAI is selected but API key is not set
    """
    if settings.embedding_type == "local":
        return LocalEmbedding(model_name=settings.local_model)

    if settings.embedding_type == "openai":
        openai_settings = get_openai_settings()
        if not openai_settings.api_key:
            raise ValueError("OpenAI API key is required when using OpenAI embeddings")
        return OpenAIEmbeddingProvider(
            api_key=openai_settings.api_key,
            model=openai_settings.model,
            base_url=openai_settings.base_url,
        )

    raise ValueError(f"Unknown embedding type: {settings.embedding_type}")


# Global embedding provider instance
_embedding_provider: EmbeddingProvider | None = None


def get_embedding_provider(settings: Settings | None = None) -> EmbeddingProvider:
    """Get or create the global embedding provider.

    Args:
        settings: Optional settings to use for creation

    Returns:
        The embedding provider instance
    """
    global _embedding_provider
    if _embedding_provider is None:
        if settings is None:
            from cangjie_mcp.config import get_settings
            settings = get_settings()
        _embedding_provider = create_embedding_provider(settings)
    return _embedding_provider


def reset_embedding_provider() -> None:
    """Reset the global embedding provider (useful for testing)."""
    global _embedding_provider
    _embedding_provider = None
