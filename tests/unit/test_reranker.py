"""Tests for indexer/reranker.py reranker providers."""

from unittest.mock import MagicMock, patch

import pytest

from cangjie_mcp.indexer.reranker import (
    LocalReranker,
    NoOpReranker,
    OpenAICompatibleReranker,
    create_reranker_provider,
    get_reranker_provider,
    reset_reranker_provider,
)


class TestNoOpReranker:
    """Tests for NoOpReranker class."""

    def test_get_model_name(self) -> None:
        """Test get_model_name returns 'none'."""
        reranker = NoOpReranker()
        assert reranker.get_model_name() == "none"

    def test_rerank_returns_unchanged(self) -> None:
        """Test rerank returns nodes unchanged."""
        reranker = NoOpReranker()

        # Create mock nodes
        mock_nodes = [MagicMock() for _ in range(5)]
        for i, node in enumerate(mock_nodes):
            node.text = f"Document {i}"
            node.score = 1.0 - i * 0.1

        result = reranker.rerank(query="test query", nodes=mock_nodes, top_k=3)  # type: ignore[arg-type]

        assert len(result) == 3
        assert result == mock_nodes[:3]

    def test_rerank_empty_nodes(self) -> None:
        """Test rerank with empty node list."""
        reranker = NoOpReranker()
        result = reranker.rerank(query="test", nodes=[], top_k=5)
        assert result == []


class TestLocalReranker:
    """Tests for LocalReranker class."""

    def test_init_default_model(self) -> None:
        """Test LocalReranker with default model."""
        reranker = LocalReranker()
        assert reranker.model_name == "BAAI/bge-reranker-v2-m3"
        assert reranker.device == "cpu"
        assert reranker._reranker is None

    def test_init_custom_model(self) -> None:
        """Test LocalReranker with custom model."""
        reranker = LocalReranker(model_name="custom-reranker", device="cuda")
        assert reranker.model_name == "custom-reranker"
        assert reranker.device == "cuda"

    def test_get_model_name(self) -> None:
        """Test get_model_name returns correct format."""
        reranker = LocalReranker(model_name="test-model")
        assert reranker.get_model_name() == "local:test-model"

    @patch("cangjie_mcp.indexer.reranker.SentenceTransformerRerank")
    def test_get_reranker_caching(self, mock_st_rerank: MagicMock) -> None:
        """Test that SentenceTransformerRerank is cached."""
        mock_reranker = MagicMock()
        mock_reranker.top_n = 5
        mock_st_rerank.return_value = mock_reranker

        reranker = LocalReranker()
        r1 = reranker._get_reranker(top_n=5)
        r2 = reranker._get_reranker(top_n=5)

        assert r1 is r2
        mock_st_rerank.assert_called_once()

    def test_rerank_empty_nodes(self) -> None:
        """Test rerank with empty node list."""
        reranker = LocalReranker()
        result = reranker.rerank(query="test", nodes=[], top_k=5)
        assert result == []


class TestOpenAICompatibleReranker:
    """Tests for OpenAICompatibleReranker class."""

    def test_init_defaults(self) -> None:
        """Test OpenAICompatibleReranker with default values."""
        reranker = OpenAICompatibleReranker(api_key="test-key")
        assert reranker.api_key == "test-key"
        assert reranker.model == "BAAI/bge-reranker-v2-m3"
        assert reranker.base_url == "https://api.openai.com/v1"

    def test_init_custom_values(self) -> None:
        """Test OpenAICompatibleReranker with custom values."""
        reranker = OpenAICompatibleReranker(
            api_key="custom-key",
            model="custom-model",
            base_url="https://custom.api.com/v1/",
        )
        assert reranker.api_key == "custom-key"
        assert reranker.model == "custom-model"
        assert reranker.base_url == "https://custom.api.com/v1"  # trailing slash stripped

    def test_get_model_name(self) -> None:
        """Test get_model_name returns correct format."""
        reranker = OpenAICompatibleReranker(api_key="test-key", model="test-model")
        assert reranker.get_model_name() == "openai:test-model"

    def test_rerank_empty_nodes(self) -> None:
        """Test rerank with empty node list."""
        reranker = OpenAICompatibleReranker(api_key="test-key")
        result = reranker.rerank(query="test", nodes=[], top_k=5)
        assert result == []

    @patch("httpx.post")
    def test_rerank_api_call(self, mock_post: MagicMock) -> None:
        """Test rerank makes correct API call."""
        mock_response = MagicMock()
        mock_response.json.return_value = {
            "results": [
                {"index": 1, "relevance_score": 0.95},
                {"index": 0, "relevance_score": 0.80},
            ]
        }
        mock_post.return_value = mock_response

        reranker = OpenAICompatibleReranker(api_key="test-key")

        # Create mock nodes
        mock_nodes = []
        for i in range(2):
            node = MagicMock()
            node.text = f"Document {i}"
            node.score = 0.5
            node.node = MagicMock()
            node.node.metadata = {}
            mock_nodes.append(node)

        result = reranker.rerank(query="test query", nodes=mock_nodes, top_k=2)

        # Verify API call
        mock_post.assert_called_once()
        call_args = mock_post.call_args
        assert "rerank" in call_args[0][0]
        assert call_args[1]["headers"]["Authorization"] == "Bearer test-key"
        assert call_args[1]["json"]["query"] == "test query"
        assert call_args[1]["json"]["documents"] == ["Document 0", "Document 1"]

        # Verify results
        assert len(result) == 2
        assert result[0].score == 0.95
        assert result[1].score == 0.80


class TestCreateRerankerProvider:
    """Tests for create_reranker_provider factory function."""

    def test_create_none_provider(self) -> None:
        """Test creating NoOp reranker provider."""
        provider = create_reranker_provider(rerank_type="none")
        assert isinstance(provider, NoOpReranker)

    def test_create_local_provider(self) -> None:
        """Test creating local reranker provider."""
        provider = create_reranker_provider(
            rerank_type="local",
            local_model="test-model",
        )
        assert isinstance(provider, LocalReranker)
        assert provider.model_name == "test-model"

    def test_create_openai_provider(self) -> None:
        """Test creating OpenAI-compatible reranker provider."""
        provider = create_reranker_provider(
            rerank_type="openai",
            api_key="test-key",
            api_model="test-model",
            api_base_url="https://custom.api.com/v1",
        )
        assert isinstance(provider, OpenAICompatibleReranker)
        assert provider.api_key == "test-key"
        assert provider.model == "test-model"
        assert provider.base_url == "https://custom.api.com/v1"

    def test_create_openai_provider_defaults(self) -> None:
        """Test creating OpenAI-compatible reranker with default model and URL."""
        provider = create_reranker_provider(
            rerank_type="openai",
            api_key="test-key",
        )
        assert isinstance(provider, OpenAICompatibleReranker)
        assert provider.model == "BAAI/bge-reranker-v2-m3"
        assert provider.base_url == "https://api.openai.com/v1"

    def test_create_openai_no_key(self) -> None:
        """Test error when OpenAI API key is not set."""
        with pytest.raises(ValueError, match="OpenAI API key is required"):
            create_reranker_provider(rerank_type="openai")

    def test_create_unknown_type(self) -> None:
        """Test error for unknown reranker type."""
        with pytest.raises(ValueError, match="Unknown rerank type"):
            create_reranker_provider(rerank_type="unknown")


class TestGetRerankerProvider:
    """Tests for get_reranker_provider global accessor."""

    def setup_method(self) -> None:
        """Reset provider before each test."""
        reset_reranker_provider()

    def teardown_method(self) -> None:
        """Reset provider after each test."""
        reset_reranker_provider()

    @patch("cangjie_mcp.config.get_settings")
    def test_get_provider_none(
        self,
        mock_get_settings: MagicMock,
    ) -> None:
        """Test get_reranker_provider with none type."""
        mock_settings = MagicMock()
        mock_settings.rerank_type = "none"
        mock_settings.rerank_model = "BAAI/bge-reranker-v2-m3"
        mock_settings.openai_api_key = None
        mock_settings.openai_base_url = None
        mock_get_settings.return_value = mock_settings

        provider = get_reranker_provider()
        assert isinstance(provider, NoOpReranker)

    @patch("cangjie_mcp.config.get_settings")
    def test_get_provider_local(
        self,
        mock_get_settings: MagicMock,
    ) -> None:
        """Test get_reranker_provider with local type."""
        mock_settings = MagicMock()
        mock_settings.rerank_type = "local"
        mock_settings.rerank_model = "custom-model"
        mock_settings.openai_api_key = None
        mock_settings.openai_base_url = None
        mock_get_settings.return_value = mock_settings

        provider = get_reranker_provider()
        assert isinstance(provider, LocalReranker)
        assert provider.model_name == "custom-model"

    @patch("cangjie_mcp.config.get_settings")
    def test_get_caching(
        self,
        mock_get_settings: MagicMock,
    ) -> None:
        """Test that provider is cached."""
        mock_settings = MagicMock()
        mock_settings.rerank_type = "none"
        mock_settings.rerank_model = "test-model"
        mock_settings.openai_api_key = None
        mock_settings.openai_base_url = None
        mock_get_settings.return_value = mock_settings

        provider1 = get_reranker_provider()
        provider2 = get_reranker_provider()

        assert provider1 is provider2


class TestResetRerankerProvider:
    """Tests for reset_reranker_provider function."""

    @patch("cangjie_mcp.config.get_settings")
    def test_reset(
        self,
        mock_get_settings: MagicMock,
    ) -> None:
        """Test resetting the global provider."""
        mock_settings = MagicMock()
        mock_settings.rerank_type = "none"
        mock_settings.rerank_model = "test-model"
        mock_settings.openai_api_key = None
        mock_settings.openai_base_url = None
        mock_get_settings.return_value = mock_settings

        provider1 = get_reranker_provider()
        reset_reranker_provider()
        provider2 = get_reranker_provider()

        # After reset, should be a new instance
        assert provider1 is not provider2
