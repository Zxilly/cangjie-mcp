"""Prebuilt index management for distribution."""

import json
import shutil
import tarfile
import tempfile
from pathlib import Path

import httpx
from pydantic import BaseModel
from rich.console import Console
from rich.progress import BarColumn, Progress, SpinnerColumn, TextColumn

console = Console()

# Metadata file inside the archive
ARCHIVE_METADATA_FILE = "prebuilt_metadata.json"


class PrebuiltMetadata(BaseModel):
    """Metadata for prebuilt index."""

    version: str
    lang: str
    embedding_model: str
    format_version: str = "1.0"


class PrebuiltArchiveInfo(BaseModel):
    """Information about a prebuilt archive."""

    version: str
    lang: str
    embedding_model: str
    path: str


class InstalledMetadata(BaseModel):
    """Metadata of installed index."""

    version: str
    lang: str
    embedding_model: str


class PrebuiltManager:
    """Manages prebuilt ChromaDB indexes for distribution."""

    def __init__(self, data_dir: Path) -> None:
        """Initialize prebuilt manager.

        Args:
            data_dir: Data directory containing chroma_db
        """
        self.data_dir = data_dir
        self.chroma_dir = data_dir / "chroma_db"
        self.prebuilt_dir = data_dir / "prebuilt"

    def build(
        self,
        version: str,
        lang: str,
        embedding_model: str,
        output_path: Path | None = None,
    ) -> Path:
        """Build a prebuilt index archive from current ChromaDB.

        Args:
            version: Documentation version
            lang: Documentation language
            embedding_model: Name of embedding model used
            output_path: Optional output path for archive

        Returns:
            Path to the created archive

        Raises:
            FileNotFoundError: If ChromaDB directory doesn't exist
        """
        if not self.chroma_dir.exists():
            raise FileNotFoundError(f"ChromaDB directory not found: {self.chroma_dir}")

        # Create prebuilt directory if needed
        self.prebuilt_dir.mkdir(parents=True, exist_ok=True)

        # Generate archive name
        archive_name = f"cangjie-index-{version}-{lang}.tar.gz"
        if output_path is None:
            output_path = self.prebuilt_dir / archive_name
        else:
            output_path = output_path / archive_name if output_path.is_dir() else output_path

        console.print(f"[blue]Building prebuilt index archive: {output_path}[/blue]")

        # Create temporary directory for packaging
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_path = Path(temp_dir)

            # Copy ChromaDB files
            temp_chroma = temp_path / "chroma_db"
            shutil.copytree(self.chroma_dir, temp_chroma)

            # Create metadata file
            metadata = {
                "version": version,
                "lang": lang,
                "embedding_model": embedding_model,
                "format_version": "1.0",
            }
            metadata_path = temp_path / ARCHIVE_METADATA_FILE
            metadata_path.write_text(json.dumps(metadata, indent=2), encoding="utf-8")

            # Create tar.gz archive
            with tarfile.open(output_path, "w:gz") as tar:
                tar.add(temp_chroma, arcname="chroma_db")
                tar.add(metadata_path, arcname=ARCHIVE_METADATA_FILE)

        console.print(f"[green]Prebuilt index created: {output_path}[/green]")
        return output_path

    def download(
        self,
        url: str,
        version: str | None = None,
        lang: str | None = None,
    ) -> Path:
        """Download a prebuilt index from URL.

        Args:
            url: URL to download from (can be a base URL or direct file URL)
            version: Optional version to construct filename
            lang: Optional language to construct filename

        Returns:
            Path to downloaded archive

        Raises:
            httpx.HTTPError: If download fails
        """
        # If version and lang provided, construct full URL
        if version and lang and not url.endswith(".tar.gz"):
            archive_name = f"cangjie-index-{version}-{lang}.tar.gz"
            url = f"{url.rstrip('/')}/{archive_name}"

        console.print(f"[blue]Downloading prebuilt index from {url}...[/blue]")

        self.prebuilt_dir.mkdir(parents=True, exist_ok=True)
        archive_name = url.split("/")[-1]
        output_path = self.prebuilt_dir / archive_name

        with httpx.Client(timeout=300.0) as client, client.stream("GET", url) as response:
            response.raise_for_status()
            total = int(response.headers.get("content-length", 0))

            with Progress(
                SpinnerColumn(),
                TextColumn("[progress.description]{task.description}"),
                BarColumn(),
                TextColumn("[progress.percentage]{task.percentage:>3.0f}%"),
                console=console,
            ) as progress:
                task = progress.add_task("Downloading...", total=total)

                with output_path.open("wb") as f:
                    for chunk in response.iter_bytes():
                        f.write(chunk)
                        progress.update(task, advance=len(chunk))

        console.print(f"[green]Downloaded to {output_path}[/green]")
        return output_path

    def install(self, archive_path: Path) -> PrebuiltMetadata:
        """Install a prebuilt index from archive.

        Args:
            archive_path: Path to the .tar.gz archive

        Returns:
            Metadata from the archive

        Raises:
            FileNotFoundError: If archive doesn't exist
            ValueError: If archive is invalid
        """
        if not archive_path.exists():
            raise FileNotFoundError(f"Archive not found: {archive_path}")

        console.print(f"[blue]Installing prebuilt index from {archive_path}...[/blue]")

        # Extract to temporary directory first
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_path = Path(temp_dir)

            with tarfile.open(archive_path, "r:gz") as tar:
                tar.extractall(temp_path, filter="data")

            # Read metadata
            metadata_path = temp_path / ARCHIVE_METADATA_FILE
            if not metadata_path.exists():
                raise ValueError("Invalid archive: missing metadata file")

            metadata = PrebuiltMetadata.model_validate_json(
                metadata_path.read_text(encoding="utf-8")
            )

            # Check for chroma_db directory
            temp_chroma = temp_path / "chroma_db"
            if not temp_chroma.exists():
                raise ValueError("Invalid archive: missing chroma_db directory")

            # Remove existing chroma_db if present
            if self.chroma_dir.exists():
                shutil.rmtree(self.chroma_dir)

            # Move extracted chroma_db to data directory
            self.data_dir.mkdir(parents=True, exist_ok=True)
            shutil.move(str(temp_chroma), str(self.chroma_dir))

            # Also copy metadata to chroma_dir for version tracking
            installed = InstalledMetadata(
                version=metadata.version,
                lang=metadata.lang,
                embedding_model=metadata.embedding_model,
            )
            index_metadata_path = self.chroma_dir / "index_metadata.json"
            index_metadata_path.write_text(installed.model_dump_json(indent=2), encoding="utf-8")

        console.print("[green]Prebuilt index installed successfully.[/green]")
        console.print(f"  Version: {metadata.version}")
        console.print(f"  Language: {metadata.lang}")
        console.print(f"  Embedding: {metadata.embedding_model}")

        return metadata

    def list_available(self, base_url: str) -> list[PrebuiltMetadata]:
        """List available prebuilt indexes from a URL.

        Args:
            base_url: Base URL to list from

        Returns:
            List of available index metadata
        """
        # This would typically fetch an index.json or similar
        # For now, return empty list - implementations can override
        index_url = f"{base_url.rstrip('/')}/index.json"
        try:
            with httpx.Client(timeout=30.0) as client:
                response = client.get(index_url)
                response.raise_for_status()
                data = response.json()
                return [PrebuiltMetadata.model_validate(item) for item in data]
        except Exception:
            return []

    def list_local(self) -> list[PrebuiltArchiveInfo]:
        """List locally available prebuilt archives.

        Returns:
            List of archive metadata
        """
        archives: list[PrebuiltArchiveInfo] = []
        if not self.prebuilt_dir.exists():
            return archives

        for archive_path in self.prebuilt_dir.glob("*.tar.gz"):
            try:
                with tarfile.open(archive_path, "r:gz") as tar:
                    metadata_file = tar.extractfile(ARCHIVE_METADATA_FILE)
                    if metadata_file:
                        metadata = PrebuiltMetadata.model_validate_json(
                            metadata_file.read().decode("utf-8")
                        )
                        archives.append(
                            PrebuiltArchiveInfo(
                                version=metadata.version,
                                lang=metadata.lang,
                                embedding_model=metadata.embedding_model,
                                path=str(archive_path),
                            )
                        )
            except Exception:
                continue

        return archives

    def get_installed_metadata(self) -> InstalledMetadata | None:
        """Get metadata of currently installed index.

        Returns:
            Metadata dict or None if not installed
        """
        metadata_path = self.chroma_dir / "index_metadata.json"
        if metadata_path.exists():
            return InstalledMetadata.model_validate_json(metadata_path.read_text(encoding="utf-8"))
        return None
