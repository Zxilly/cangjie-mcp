"""Tests for indexer/embeddings.py embedding providers."""

from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

from cangjie_mcp.config import Settings
from cangjie_mcp.indexer.embeddings import (
    LocalEmbedding,
    OpenAIEmbeddingProvider,
    create_embedding_provider,
    get_embedding_provider,
    reset_embedding_provider,
)


class TestLocalEmbedding:
    """Tests for LocalEmbedding class."""

    def test_init_default_model(self) -> None:
        """Test LocalEmbedding with default model."""
        provider = LocalEmbedding()
        assert provider.model_name == "paraphrase-multilingual-MiniLM-L12-v2"
        assert provider._model is None

    def test_init_custom_model(self) -> None:
        """Test LocalEmbedding with custom model."""
        provider = LocalEmbedding(model_name="custom-model")
        assert provider.model_name == "custom-model"

    def test_get_model_name(self) -> None:
        """Test get_model_name returns correct format."""
        provider = LocalEmbedding(model_name="test-model")
        assert provider.get_model_name() == "local:test-model"

    @patch("cangjie_mcp.indexer.embeddings.HuggingFaceEmbedding")
    def test_get_embedding_model_caching(self, mock_hf_embedding: MagicMock) -> None:
        """Test that embedding model is cached."""
        mock_model = MagicMock()
        mock_hf_embedding.return_value = mock_model

        provider = LocalEmbedding()
        model1 = provider.get_embedding_model()
        model2 = provider.get_embedding_model()

        assert model1 is model2
        mock_hf_embedding.assert_called_once()


class TestOpenAIEmbeddingProvider:
    """Tests for OpenAIEmbeddingProvider class."""

    def test_init(self) -> None:
        """Test OpenAIEmbeddingProvider initialization."""
        provider = OpenAIEmbeddingProvider(
            api_key="test-key",
            model="text-embedding-3-large",
            base_url="https://custom.api.com/v1",
        )
        assert provider.api_key == "test-key"
        assert provider.model == "text-embedding-3-large"
        assert provider.base_url == "https://custom.api.com/v1"

    def test_init_defaults(self) -> None:
        """Test OpenAIEmbeddingProvider with default values."""
        provider = OpenAIEmbeddingProvider(api_key="test-key")
        assert provider.model == "text-embedding-3-small"
        assert provider.base_url == "https://api.openai.com/v1"

    def test_get_model_name(self) -> None:
        """Test get_model_name returns correct format."""
        provider = OpenAIEmbeddingProvider(api_key="test-key", model="ada-002")
        assert provider.get_model_name() == "openai:ada-002"

    @patch("cangjie_mcp.indexer.embeddings.OpenAIEmbedding")
    def test_get_embedding_model_caching(self, mock_openai_embedding: MagicMock) -> None:
        """Test that embedding model is cached."""
        mock_model = MagicMock()
        mock_openai_embedding.return_value = mock_model

        provider = OpenAIEmbeddingProvider(api_key="test-key")
        model1 = provider.get_embedding_model()
        model2 = provider.get_embedding_model()

        assert model1 is model2
        mock_openai_embedding.assert_called_once()


class TestCreateEmbeddingProvider:
    """Tests for create_embedding_provider factory function."""

    def test_create_local_provider(self, temp_data_dir: Path) -> None:
        """Test creating local embedding provider."""
        settings = Settings(
            embedding_type="local",
            local_model="test-local-model",
            data_dir=temp_data_dir,
        )

        provider = create_embedding_provider(settings)

        assert isinstance(provider, LocalEmbedding)
        assert provider.model_name == "test-local-model"

    def test_create_openai_provider(self, temp_data_dir: Path) -> None:
        """Test creating OpenAI embedding provider."""
        settings = Settings(
            embedding_type="openai",
            openai_api_key="test-api-key",
            openai_model="text-embedding-3-small",
            openai_base_url="https://api.openai.com/v1",
            data_dir=temp_data_dir,
        )

        provider = create_embedding_provider(settings)

        assert isinstance(provider, OpenAIEmbeddingProvider)
        assert provider.api_key == "test-api-key"
        assert provider.model == "text-embedding-3-small"

    def test_create_openai_provider_no_key(self, temp_data_dir: Path) -> None:
        """Test error when OpenAI key is not set."""
        settings = Settings(
            embedding_type="openai",
            openai_api_key=None,
            data_dir=temp_data_dir,
        )

        with pytest.raises(ValueError, match="OpenAI API key is required"):
            create_embedding_provider(settings)


class TestGetEmbeddingProvider:
    """Tests for get_embedding_provider global accessor."""

    def setup_method(self) -> None:
        """Reset provider before each test."""
        reset_embedding_provider()

    def teardown_method(self) -> None:
        """Reset provider after each test."""
        reset_embedding_provider()

    def test_get_with_settings(self, temp_data_dir: Path) -> None:
        """Test get_embedding_provider with explicit settings."""
        settings = Settings(
            embedding_type="local",
            local_model="test-model",
            data_dir=temp_data_dir,
        )

        provider = get_embedding_provider(settings)

        assert isinstance(provider, LocalEmbedding)
        assert provider.model_name == "test-model"

    def test_get_caching(self, temp_data_dir: Path) -> None:
        """Test that provider is cached."""
        settings = Settings(
            embedding_type="local",
            data_dir=temp_data_dir,
        )

        provider1 = get_embedding_provider(settings)
        provider2 = get_embedding_provider(settings)

        assert provider1 is provider2


class TestResetEmbeddingProvider:
    """Tests for reset_embedding_provider function."""

    def test_reset(self, temp_data_dir: Path) -> None:
        """Test resetting the global provider."""
        settings = Settings(
            embedding_type="local",
            data_dir=temp_data_dir,
        )

        provider1 = get_embedding_provider(settings)
        reset_embedding_provider()
        provider2 = get_embedding_provider(settings)

        # After reset, should be a new instance
        assert provider1 is not provider2
