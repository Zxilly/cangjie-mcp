"""End-to-end integration tests simulating real usage.

These tests verify complete workflows from document loading
through search and tool usage.
"""

from pathlib import Path
from unittest.mock import MagicMock

import pytest
from mcp.server.fastmcp import Context

from cangjie_mcp.config import IndexInfo, Settings
from cangjie_mcp.indexer.loader import DocumentLoader
from cangjie_mcp.indexer.store import VectorStore
from cangjie_mcp.server import tools
from tests.integration.conftest import TestDocumentSource, VectorStoreSearchIndex


def _mock_ctx(tool_context: tools.ToolContext) -> MagicMock:
    """Create a mock MCP Context wrapping a ToolContext."""
    ctx = MagicMock(spec=Context)
    lifespan_ctx = tools.LifespanContext()
    lifespan_ctx.complete(tool_context)
    ctx.request_context.lifespan_context = lifespan_ctx
    return ctx


class TestEndToEndWorkflow:
    """End-to-end integration tests simulating real usage."""

    def test_complete_search_workflow(
        self,
        local_indexed_store: VectorStore,
    ) -> None:
        """Test search workflow using the shared indexed store."""
        results = local_indexed_store.search(query="模式匹配", top_k=3)
        assert len(results) > 0
        assert any("match" in r.text.lower() or "模式" in r.text for r in results)

    @pytest.mark.asyncio
    async def test_tool_workflow_with_mcp_server(
        self,
        test_doc_source: TestDocumentSource,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """Test using tools through ToolContext."""
        ctx = _mock_ctx(
            tools.ToolContext(
                settings=shared_local_settings,
                index_info=IndexInfo.from_settings(shared_local_settings),
                search_index=VectorStoreSearchIndex(local_indexed_store),
                document_source=test_doc_source,
            )
        )

        result = await tools.list_topics(ctx=ctx)
        assert result.total_topics > 0

        for category, topic_list in result.categories.items():
            if topic_list:
                topic = topic_list[0]
                doc = await tools.get_topic(topic=topic, category=category, ctx=ctx)
                assert doc is not None
                break

        search_results = await tools.search_docs(query="仓颉语言", top_k=5, ctx=ctx)
        assert search_results.count > 0

    @pytest.mark.asyncio
    async def test_category_based_exploration(
        self,
        test_doc_source: TestDocumentSource,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """Test exploring documentation by category."""
        ctx = _mock_ctx(
            tools.ToolContext(
                settings=shared_local_settings,
                index_info=IndexInfo.from_settings(shared_local_settings),
                search_index=VectorStoreSearchIndex(local_indexed_store),
                document_source=test_doc_source,
            )
        )

        result = await tools.list_topics(ctx=ctx)

        for category in result.categories:
            filtered = await tools.list_topics(category=category, ctx=ctx)
            assert category in filtered.categories
            assert len(filtered.categories[category]) > 0

            search_results = await tools.search_docs(query="使用方法", category=category, top_k=2, ctx=ctx)
            if search_results.count > 0:
                assert all(r.category == category for r in search_results.items)

    @pytest.mark.asyncio
    async def test_full_document_discovery_workflow(
        self,
        test_doc_source: TestDocumentSource,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """Test complete document discovery workflow."""
        ctx = _mock_ctx(
            tools.ToolContext(
                settings=shared_local_settings,
                index_info=IndexInfo.from_settings(shared_local_settings),
                search_index=VectorStoreSearchIndex(local_indexed_store),
                document_source=test_doc_source,
            )
        )

        # 1. List all topics
        result = await tools.list_topics(ctx=ctx)
        assert result.total_categories > 0

        # 2. Get topics count
        assert result.total_topics == 6  # We have 6 test documents

        # 3. Read each topic
        for category, topic_list in result.categories.items():
            for topic in topic_list:
                doc = await tools.get_topic(topic=topic, category=category, ctx=ctx)
                assert doc is not None
                assert doc.category == category
                assert doc.topic == topic
                assert len(doc.content) > 0

    @pytest.mark.asyncio
    async def test_search_and_retrieve_workflow(
        self,
        test_doc_source: TestDocumentSource,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """Test search followed by document retrieval."""
        ctx = _mock_ctx(
            tools.ToolContext(
                settings=shared_local_settings,
                index_info=IndexInfo.from_settings(shared_local_settings),
                search_index=VectorStoreSearchIndex(local_indexed_store),
                document_source=test_doc_source,
            )
        )

        # Search for a topic
        results = await tools.search_docs(query="函数定义", top_k=3, ctx=ctx)
        assert results.count > 0

        # Get the top result's topic
        top_result = results.items[0]
        topic = top_result.topic
        category = top_result.category

        # Retrieve full document
        doc = await tools.get_topic(topic=topic, category=category, ctx=ctx)
        assert doc is not None
        assert len(doc.content) >= len(top_result.content)

    @pytest.mark.asyncio
    async def test_code_examples_workflow(
        self,
        test_doc_source: TestDocumentSource,
        local_indexed_store: VectorStore,
        shared_local_settings: Settings,
    ) -> None:
        """Test finding and using code examples."""
        ctx = _mock_ctx(
            tools.ToolContext(
                settings=shared_local_settings,
                index_info=IndexInfo.from_settings(shared_local_settings),
                search_index=VectorStoreSearchIndex(local_indexed_store),
                document_source=test_doc_source,
            )
        )

        # Get code examples for a feature
        examples = await tools.get_code_examples(feature="Hello World", top_k=5, ctx=ctx)
        assert len(examples) > 0

        # Verify examples have required fields
        for example in examples:
            assert isinstance(example.language, str)
            assert isinstance(example.code, str)
            assert len(example.code) > 0

        # Get tool usage
        tool_result = await tools.get_tool_usage(tool_name="cjc", ctx=ctx)
        assert tool_result is not None
        assert isinstance(tool_result.examples, list)

    def test_indexing_preserves_document_structure(
        self,
        integration_docs_dir: Path,
        local_settings: Settings,
        shared_embedding_provider,
    ) -> None:
        """Test that indexing preserves document metadata correctly."""
        loader = DocumentLoader(integration_docs_dir)
        documents = loader.load_all_documents()

        # Verify documents have correct metadata
        categories = {doc.metadata.get("category") for doc in documents}
        assert "basics" in categories
        assert "syntax" in categories
        assert "tools" in categories

        # Index and search
        store = VectorStore(
            db_path=IndexInfo.from_settings(local_settings).chroma_db_dir,
            embedding_provider=shared_embedding_provider,
        )
        store.index_documents(documents)

        # Verify search results maintain metadata
        results = store.search(query="仓颉", top_k=10)
        for result in results:
            assert result.metadata.category in categories
            assert result.metadata.topic != ""

    def test_multilingual_search(
        self,
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
