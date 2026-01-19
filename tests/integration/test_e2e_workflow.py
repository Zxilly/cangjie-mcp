"""End-to-end integration tests simulating real usage.

These tests verify complete workflows from document loading
through search and tool usage.
"""

from pathlib import Path

from cangjie_mcp.config import Settings
from cangjie_mcp.indexer.embeddings import get_embedding_provider, reset_embedding_provider
from cangjie_mcp.indexer.loader import DocumentLoader
from cangjie_mcp.indexer.store import VectorStore
from cangjie_mcp.server import tools
from cangjie_mcp.server.tools import (
    GetCodeExamplesInput,
    GetToolUsageInput,
    GetTopicInput,
    ListTopicsInput,
    SearchDocsInput,
)


class TestEndToEndWorkflow:
    """End-to-end integration tests simulating real usage."""

    def test_complete_search_workflow(
        self,
        integration_docs_dir: Path,
        local_settings: Settings,
    ) -> None:
        """Test complete workflow from fresh start to search results."""
        reset_embedding_provider()
        loader = DocumentLoader(integration_docs_dir)
        documents = loader.load_all_documents()
        assert len(documents) > 0

        embedding_provider = get_embedding_provider(local_settings)
        store = VectorStore(
            db_path=local_settings.chroma_db_dir,
            embedding_provider=embedding_provider,
        )
        store.index_documents(documents)

        results = store.search(query="模式匹配", top_k=3)
        assert len(results) > 0
        assert any("match" in r.text.lower() or "模式" in r.text for r in results)

    def test_tool_workflow_with_mcp_server(
        self,
        integration_docs_dir: Path,
        local_indexed_store: VectorStore,
        local_settings: Settings,
    ) -> None:
        """Test using tools through ToolContext."""
        loader = DocumentLoader(integration_docs_dir)
        ctx = tools.ToolContext(
            settings=local_settings,
            store=local_indexed_store,
            loader=loader,
        )

        result = tools.list_topics(ctx, ListTopicsInput())
        assert result["total_topics"] > 0

        for category, topic_list in result["categories"].items():
            if topic_list:
                topic = topic_list[0]
                doc = tools.get_topic(ctx, GetTopicInput(topic=topic, category=category))
                assert doc is not None
                break

        search_results = tools.search_docs(ctx, SearchDocsInput(query="仓颉语言", top_k=5))
        assert search_results["count"] > 0

    def test_category_based_exploration(
        self,
        integration_docs_dir: Path,
        local_indexed_store: VectorStore,
        local_settings: Settings,
    ) -> None:
        """Test exploring documentation by category."""
        loader = DocumentLoader(integration_docs_dir)
        ctx = tools.ToolContext(
            settings=local_settings,
            store=local_indexed_store,
            loader=loader,
        )

        result = tools.list_topics(ctx, ListTopicsInput())

        for category in result["categories"]:
            filtered = tools.list_topics(ctx, ListTopicsInput(category=category))
            assert category in filtered["categories"]
            assert len(filtered["categories"][category]) > 0

            search_results = tools.search_docs(
                ctx,
                SearchDocsInput(query="使用方法", category=category, top_k=2),
            )
            if search_results["count"] > 0:
                assert all(r["category"] == category for r in search_results["items"])

    def test_full_document_discovery_workflow(
        self,
        integration_docs_dir: Path,
        local_indexed_store: VectorStore,
        local_settings: Settings,
    ) -> None:
        """Test complete document discovery workflow."""
        loader = DocumentLoader(integration_docs_dir)
        ctx = tools.ToolContext(
            settings=local_settings,
            store=local_indexed_store,
            loader=loader,
        )

        # 1. List all topics
        result = tools.list_topics(ctx, ListTopicsInput())
        assert result["total_categories"] > 0

        # 2. Get topics count
        assert result["total_topics"] == 6  # We have 6 test documents

        # 3. Read each topic
        for category, topic_list in result["categories"].items():
            for topic in topic_list:
                doc = tools.get_topic(ctx, GetTopicInput(topic=topic, category=category))
                assert doc is not None
                assert doc["category"] == category
                assert doc["topic"] == topic
                assert len(doc["content"]) > 0

    def test_search_and_retrieve_workflow(
        self,
        integration_docs_dir: Path,
        local_indexed_store: VectorStore,
        local_settings: Settings,
    ) -> None:
        """Test search followed by document retrieval."""
        loader = DocumentLoader(integration_docs_dir)
        ctx = tools.ToolContext(
            settings=local_settings,
            store=local_indexed_store,
            loader=loader,
        )

        # Search for a topic
        results = tools.search_docs(ctx, SearchDocsInput(query="函数定义", top_k=3))
        assert results["count"] > 0

        # Get the top result's topic
        top_result = results["items"][0]
        topic = top_result["topic"]
        category = top_result["category"]

        # Retrieve full document
        doc = tools.get_topic(ctx, GetTopicInput(topic=topic, category=category))
        assert doc is not None
        assert len(doc["content"]) >= len(top_result["content"])

    def test_code_examples_workflow(
        self,
        integration_docs_dir: Path,
        local_indexed_store: VectorStore,
        local_settings: Settings,
    ) -> None:
        """Test finding and using code examples."""
        loader = DocumentLoader(integration_docs_dir)
        ctx = tools.ToolContext(
            settings=local_settings,
            store=local_indexed_store,
            loader=loader,
        )

        # Get code examples for a feature
        examples = tools.get_code_examples(
            ctx, GetCodeExamplesInput(feature="Hello World", top_k=5)
        )
        assert len(examples) > 0

        # Verify examples have required fields
        for example in examples:
            assert "language" in example
            assert "code" in example
            assert len(example["code"]) > 0

        # Get tool usage
        tool_result = tools.get_tool_usage(ctx, GetToolUsageInput(tool_name="cjc"))
        assert tool_result is not None
        assert "examples" in tool_result

    def test_indexing_preserves_document_structure(
        self,
        integration_docs_dir: Path,
        local_settings: Settings,
    ) -> None:
        """Test that indexing preserves document metadata correctly."""
        reset_embedding_provider()
        loader = DocumentLoader(integration_docs_dir)
        documents = loader.load_all_documents()

        # Verify documents have correct metadata
        categories = {doc.metadata.get("category") for doc in documents}
        assert "basics" in categories
        assert "syntax" in categories
        assert "tools" in categories

        # Index and search
        embedding_provider = get_embedding_provider(local_settings)
        store = VectorStore(
            db_path=local_settings.chroma_db_dir,
            embedding_provider=embedding_provider,
        )
        store.index_documents(documents)

        # Verify search results maintain metadata
        results = store.search(query="仓颉", top_k=10)
        for result in results:
            assert result.metadata.category in categories
            assert result.metadata.topic != ""

    def test_multilingual_search(
        self,
        integration_docs_dir: Path,
        local_indexed_store: VectorStore,
    ) -> None:
        """Test searching with Chinese and English queries."""
        # Chinese query
        zh_results = local_indexed_store.search(query="函数", top_k=3)
        assert len(zh_results) > 0

        # English-like query (code keywords)
        en_results = local_indexed_store.search(query="func main", top_k=3)
        assert len(en_results) > 0

        # Mixed query
        mixed_results = local_indexed_store.search(query="Hello 世界", top_k=3)
        assert len(mixed_results) > 0
