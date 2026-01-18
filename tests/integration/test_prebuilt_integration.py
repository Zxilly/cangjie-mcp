"""Integration tests for prebuilt index functionality.

These tests verify building, installing, and using prebuilt indexes.
"""

from pathlib import Path

from cangjie_mcp.config import Settings
from cangjie_mcp.indexer.embeddings import get_embedding_provider, reset_embedding_provider
from cangjie_mcp.indexer.loader import DocumentLoader
from cangjie_mcp.indexer.store import VectorStore
from cangjie_mcp.prebuilt.manager import PrebuiltManager

from tests.constants import CANGJIE_DOCS_VERSION, CANGJIE_LOCAL_MODEL


class TestPrebuiltIndexIntegration:
    """Integration tests for prebuilt index functionality."""

    def test_build_and_install_prebuilt_index(
        self,
        integration_docs_dir: Path,
        local_settings: Settings,
    ) -> None:
        """Test building and installing a prebuilt index with local embeddings."""
        reset_embedding_provider()

        # First, create an indexed store
        embedding_provider = get_embedding_provider(local_settings)
        store = VectorStore(
            db_path=local_settings.chroma_db_dir,
            embedding_provider=embedding_provider,
        )

        loader = DocumentLoader(integration_docs_dir)
        documents = loader.load_all_documents()
        store.index_documents(documents)
        store.save_metadata(
            version=CANGJIE_DOCS_VERSION,
            lang="zh",
            embedding_model=CANGJIE_LOCAL_MODEL,
        )

        # Build prebuilt archive - use index_dir which contains chroma_db
        manager = PrebuiltManager(local_settings.index_dir)
        archive_path = manager.build(
            version=CANGJIE_DOCS_VERSION,
            lang="zh",
            embedding_model=CANGJIE_LOCAL_MODEL,
        )

        assert archive_path.exists()
        assert archive_path.suffix == ".gz"

        # List local archives
        archives = manager.list_local()
        assert len(archives) == 1
        assert archives[0].version == CANGJIE_DOCS_VERSION
        assert archives[0].lang == "zh"
        assert archives[0].embedding_model == CANGJIE_LOCAL_MODEL

    def test_install_and_load_prebuilt_index(
        self,
        integration_docs_dir: Path,
        temp_data_dir: Path,
    ) -> None:
        """Test installing and loading a prebuilt index."""
        reset_embedding_provider()

        # Create settings for the first index
        settings1 = Settings(
            docs_version=CANGJIE_DOCS_VERSION,
            docs_lang="zh",
            embedding_type="local",
            local_model=CANGJIE_LOCAL_MODEL,
            data_dir=temp_data_dir / "source",
        )

        # Create and index documents
        embedding_provider1 = get_embedding_provider(settings1)
        store1 = VectorStore(
            db_path=settings1.chroma_db_dir,
            embedding_provider=embedding_provider1,
        )

        loader = DocumentLoader(integration_docs_dir)
        documents = loader.load_all_documents()
        store1.index_documents(documents)
        store1.save_metadata(
            version=CANGJIE_DOCS_VERSION,
            lang="zh",
            embedding_model=CANGJIE_LOCAL_MODEL,
        )

        # Build prebuilt archive - use index_dir which contains chroma_db
        manager1 = PrebuiltManager(settings1.index_dir)
        archive_path = manager1.build(
            version=CANGJIE_DOCS_VERSION,
            lang="zh",
            embedding_model=CANGJIE_LOCAL_MODEL,
        )

        # Install to a different location (target index_dir)
        reset_embedding_provider()
        target_settings = Settings(
            docs_version=CANGJIE_DOCS_VERSION,
            docs_lang="zh",
            embedding_type="local",
            local_model=CANGJIE_LOCAL_MODEL,
            data_dir=temp_data_dir / "target",
        )
        manager2 = PrebuiltManager(target_settings.index_dir)
        metadata = manager2.install(archive_path)

        assert metadata.version == CANGJIE_DOCS_VERSION
        assert metadata.lang == "zh"
        assert metadata.embedding_model == CANGJIE_LOCAL_MODEL

        # Verify installed metadata
        installed = manager2.get_installed_metadata()
        assert installed is not None
        assert installed.version == CANGJIE_DOCS_VERSION

        # Load and search the installed index
        embedding_provider2 = get_embedding_provider(target_settings)
        store2 = VectorStore(
            db_path=target_settings.chroma_db_dir,
            embedding_provider=embedding_provider2,
        )

        # Verify we can search the installed index
        results = store2.search(query="Hello World", top_k=3)
        assert len(results) > 0

    def test_prebuilt_archive_metadata_roundtrip(
        self,
        integration_docs_dir: Path,
        temp_data_dir: Path,
    ) -> None:
        """Test that prebuilt archive metadata survives install/export cycle."""
        reset_embedding_provider()

        settings = Settings(
            docs_version=CANGJIE_DOCS_VERSION,
            docs_lang="zh",
            embedding_type="local",
            local_model=CANGJIE_LOCAL_MODEL,
            data_dir=temp_data_dir / "source",
        )

        # Create and index
        embedding_provider = get_embedding_provider(settings)
        store = VectorStore(
            db_path=settings.chroma_db_dir,
            embedding_provider=embedding_provider,
        )

        loader = DocumentLoader(integration_docs_dir)
        documents = loader.load_all_documents()
        store.index_documents(documents)
        store.save_metadata(
            version=CANGJIE_DOCS_VERSION,
            lang="zh",
            embedding_model=CANGJIE_LOCAL_MODEL,
        )

        # Build archive - use index_dir which contains chroma_db
        manager = PrebuiltManager(settings.index_dir)
        archive_path = manager.build(
            version=CANGJIE_DOCS_VERSION,
            lang="zh",
            embedding_model=CANGJIE_LOCAL_MODEL,
        )

        # Install to new location
        reset_embedding_provider()
        target_settings = Settings(
            docs_version=CANGJIE_DOCS_VERSION,
            docs_lang="zh",
            embedding_type="local",
            local_model=CANGJIE_LOCAL_MODEL,
            data_dir=temp_data_dir / "target",
        )
        target_manager = PrebuiltManager(target_settings.index_dir)
        installed_meta = target_manager.install(archive_path)

        # Verify all metadata preserved
        assert installed_meta.version == CANGJIE_DOCS_VERSION
        assert installed_meta.lang == "zh"
        assert installed_meta.embedding_model == CANGJIE_LOCAL_MODEL

    def test_multiple_archives_listing(
        self,
        integration_docs_dir: Path,
        temp_data_dir: Path,
    ) -> None:
        """Test listing multiple prebuilt archives."""
        reset_embedding_provider()

        # Create first index
        settings1 = Settings(
            docs_version="v1.0.0",
            docs_lang="zh",
            embedding_type="local",
            local_model=CANGJIE_LOCAL_MODEL,
            data_dir=temp_data_dir / "source1",
        )

        embedding_provider1 = get_embedding_provider(settings1)
        store1 = VectorStore(
            db_path=settings1.chroma_db_dir,
            embedding_provider=embedding_provider1,
        )

        loader = DocumentLoader(integration_docs_dir)
        documents = loader.load_all_documents()
        store1.index_documents(documents)
        store1.save_metadata(
            version="v1.0.0",
            lang="zh",
            embedding_model=CANGJIE_LOCAL_MODEL,
        )

        # Use index_dir which contains chroma_db
        manager = PrebuiltManager(settings1.index_dir)
        archive1 = manager.build(
            version="v1.0.0",
            lang="zh",
            embedding_model=CANGJIE_LOCAL_MODEL,
        )

        # Create second index
        reset_embedding_provider()
        settings2 = Settings(
            docs_version="v2.0.0",
            docs_lang="en",
            embedding_type="local",
            local_model=CANGJIE_LOCAL_MODEL,
            data_dir=temp_data_dir / "source2",
        )

        embedding_provider2 = get_embedding_provider(settings2)
        store2 = VectorStore(
            db_path=settings2.chroma_db_dir,
            embedding_provider=embedding_provider2,
        )

        store2.index_documents(documents)
        store2.save_metadata(
            version="v2.0.0",
            lang="en",
            embedding_model=CANGJIE_LOCAL_MODEL,
        )

        # Use index_dir which contains chroma_db
        manager2 = PrebuiltManager(settings2.index_dir)
        archive2 = manager2.build(
            version="v2.0.0",
            lang="en",
            embedding_model=CANGJIE_LOCAL_MODEL,
        )

        # Copy archives to common prebuilt dir
        common_prebuilt = temp_data_dir / "common" / "prebuilt"
        common_prebuilt.mkdir(parents=True)

        import shutil

        shutil.copy(archive1, common_prebuilt)
        shutil.copy(archive2, common_prebuilt)

        # List archives from common location
        common_manager = PrebuiltManager(temp_data_dir / "common")
        archives = common_manager.list_local()

        assert len(archives) == 2
        versions = {a.version for a in archives}
        assert "v1.0.0" in versions
        assert "v2.0.0" in versions
