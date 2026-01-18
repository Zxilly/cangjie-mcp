"""Tests for prebuilt/manager.py Pydantic models and PrebuiltManager."""

import json
import tarfile
from pathlib import Path

import pytest

from cangjie_mcp.prebuilt.manager import (
    InstalledMetadata,
    PrebuiltArchiveInfo,
    PrebuiltManager,
    PrebuiltMetadata,
)


class TestPrebuiltMetadata:
    """Tests for PrebuiltMetadata Pydantic model."""

    def test_create_metadata(self) -> None:
        """Test creating PrebuiltMetadata."""
        metadata = PrebuiltMetadata(
            version="0.53.18",
            lang="zh",
            embedding_model="BAAI/bge-small-zh-v1.5",
        )
        assert metadata.version == "0.53.18"
        assert metadata.lang == "zh"
        assert metadata.embedding_model == "BAAI/bge-small-zh-v1.5"
        assert metadata.format_version == "1.0"

    def test_custom_format_version(self) -> None:
        """Test with custom format version."""
        metadata = PrebuiltMetadata(
            version="latest",
            lang="en",
            embedding_model="text-embedding-3-small",
            format_version="2.0",
        )
        assert metadata.format_version == "2.0"

    def test_json_roundtrip(self) -> None:
        """Test JSON serialization and deserialization."""
        metadata = PrebuiltMetadata(
            version="0.53.18",
            lang="zh",
            embedding_model="BAAI/bge-small-zh-v1.5",
        )
        json_str = metadata.model_dump_json()
        parsed = PrebuiltMetadata.model_validate_json(json_str)
        assert parsed.version == metadata.version
        assert parsed.lang == metadata.lang
        assert parsed.embedding_model == metadata.embedding_model
        assert parsed.format_version == metadata.format_version


class TestPrebuiltArchiveInfo:
    """Tests for PrebuiltArchiveInfo Pydantic model."""

    def test_create_info(self) -> None:
        """Test creating PrebuiltArchiveInfo."""
        info = PrebuiltArchiveInfo(
            version="0.53.18",
            lang="zh",
            embedding_model="BAAI/bge-small-zh-v1.5",
            path="/data/prebuilt/cangjie-index-0.53.18-zh.tar.gz",
        )
        assert info.version == "0.53.18"
        assert info.lang == "zh"
        assert info.embedding_model == "BAAI/bge-small-zh-v1.5"
        assert info.path == "/data/prebuilt/cangjie-index-0.53.18-zh.tar.gz"

    def test_attribute_access(self) -> None:
        """Test attribute access pattern (replaces .get() pattern)."""
        info = PrebuiltArchiveInfo(
            version="latest",
            lang="en",
            embedding_model="text-embedding-3-small",
            path="/prebuilt/index.tar.gz",
        )
        # This pattern is used in cli.py instead of info.get("version")
        assert info.version == "latest"
        assert info.lang == "en"
        assert info.embedding_model == "text-embedding-3-small"
        assert info.path == "/prebuilt/index.tar.gz"


class TestInstalledMetadata:
    """Tests for InstalledMetadata Pydantic model."""

    def test_create_metadata(self) -> None:
        """Test creating InstalledMetadata."""
        metadata = InstalledMetadata(
            version="0.53.18",
            lang="zh",
            embedding_model="BAAI/bge-small-zh-v1.5",
        )
        assert metadata.version == "0.53.18"
        assert metadata.lang == "zh"
        assert metadata.embedding_model == "BAAI/bge-small-zh-v1.5"

    def test_json_file_roundtrip(self, temp_data_dir: Path) -> None:
        """Test saving and loading from JSON file."""
        metadata = InstalledMetadata(
            version="latest",
            lang="en",
            embedding_model="text-embedding-3-small",
        )
        file_path = temp_data_dir / "installed.json"
        file_path.write_text(metadata.model_dump_json(indent=2), encoding="utf-8")

        loaded = InstalledMetadata.model_validate_json(file_path.read_text(encoding="utf-8"))
        assert loaded.version == "latest"
        assert loaded.lang == "en"
        assert loaded.embedding_model == "text-embedding-3-small"


class TestPrebuiltManager:
    """Tests for PrebuiltManager class."""

    def test_init(self, temp_data_dir: Path) -> None:
        """Test manager initialization."""
        mgr = PrebuiltManager(temp_data_dir)
        assert mgr.data_dir == temp_data_dir
        assert mgr.chroma_dir == temp_data_dir / "chroma_db"
        assert mgr.prebuilt_dir == temp_data_dir / "prebuilt"

    def test_build_no_chroma(self, temp_data_dir: Path) -> None:
        """Test build fails when ChromaDB doesn't exist."""
        mgr = PrebuiltManager(temp_data_dir)
        with pytest.raises(FileNotFoundError, match="ChromaDB directory not found"):
            mgr.build(version="0.53.18", lang="zh", embedding_model="test")

    def test_build_and_list_local(self, temp_data_dir: Path) -> None:
        """Test building and listing local archives."""
        mgr = PrebuiltManager(temp_data_dir)

        # Create mock chroma_db
        chroma_dir = temp_data_dir / "chroma_db"
        chroma_dir.mkdir(parents=True)
        (chroma_dir / "test.db").write_text("mock data", encoding="utf-8")

        # Build archive
        archive_path = mgr.build(
            version="0.53.18",
            lang="zh",
            embedding_model="BAAI/bge-small-zh-v1.5",
        )

        assert archive_path.exists()
        assert archive_path.name == "cangjie-index-0.53.18-zh.tar.gz"

        # List local archives
        archives = mgr.list_local()
        assert len(archives) == 1
        assert archives[0].version == "0.53.18"
        assert archives[0].lang == "zh"

    def test_install(self, temp_data_dir: Path) -> None:
        """Test installing from archive."""
        mgr = PrebuiltManager(temp_data_dir)

        # Create a test archive manually
        prebuilt_dir = temp_data_dir / "prebuilt"
        prebuilt_dir.mkdir(parents=True)
        archive_path = prebuilt_dir / "test-index.tar.gz"

        # Create archive contents in temp location
        temp_content = temp_data_dir / "temp_content"
        temp_content.mkdir()
        (temp_content / "chroma_db").mkdir()
        (temp_content / "chroma_db" / "data.db").write_text("mock", encoding="utf-8")

        metadata = {
            "version": "0.53.18",
            "lang": "zh",
            "embedding_model": "BAAI/bge-small-zh-v1.5",
            "format_version": "1.0",
        }
        (temp_content / "prebuilt_metadata.json").write_text(
            json.dumps(metadata), encoding="utf-8"
        )

        # Create tar.gz
        with tarfile.open(archive_path, "w:gz") as tar:
            tar.add(temp_content / "chroma_db", arcname="chroma_db")
            tar.add(temp_content / "prebuilt_metadata.json", arcname="prebuilt_metadata.json")

        # Install
        result = mgr.install(archive_path)
        assert result.version == "0.53.18"
        assert result.lang == "zh"

        # Check installed metadata
        installed = mgr.get_installed_metadata()
        assert installed is not None
        assert installed.version == "0.53.18"

    def test_get_installed_metadata_not_installed(self, temp_data_dir: Path) -> None:
        """Test get_installed_metadata when nothing is installed."""
        mgr = PrebuiltManager(temp_data_dir)
        assert mgr.get_installed_metadata() is None

    def test_list_local_empty(self, temp_data_dir: Path) -> None:
        """Test list_local when no archives exist."""
        mgr = PrebuiltManager(temp_data_dir)
        assert mgr.list_local() == []
