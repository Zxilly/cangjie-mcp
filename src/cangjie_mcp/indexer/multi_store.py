"""Multi-index management for loading multiple VectorStores from URLs."""

from __future__ import annotations

import hashlib
import shutil
import tarfile
import tempfile
from pathlib import Path
from typing import TYPE_CHECKING

import httpx
from rich.console import Console

from cangjie_mcp.config import IndexKey, Settings
from cangjie_mcp.indexer.embeddings import create_embedding_provider
from cangjie_mcp.indexer.reranker import get_reranker_provider
from cangjie_mcp.indexer.store import VectorStore
from cangjie_mcp.prebuilt.manager import ARCHIVE_METADATA_FILE, PrebuiltMetadata
from cangjie_mcp.utils import create_download_progress

if TYPE_CHECKING:
    from cangjie_mcp.indexer.embeddings import EmbeddingProvider

console = Console()


def _url_to_cache_key(url: str) -> str:
    """Generate MD5 cache key from URL.

    Args:
        url: The URL to hash

    Returns:
        MD5 hex digest of the URL
    """
    return hashlib.md5(url.encode("utf-8")).hexdigest()


class LoadedIndex:
    """Represents a loaded index with its metadata and store."""

    def __init__(
        self,
        url: str,
        metadata: PrebuiltMetadata,
        store: VectorStore,
        key: IndexKey,
    ) -> None:
        self.url = url
        self.metadata = metadata
        self.store = store
        self.key = key


class MultiIndexStore:
    """Manages multiple VectorStore instances loaded from URLs.

    This class downloads prebuilt index archives from URLs, caches them
    using the URL's MD5 hash, and creates VectorStore instances for each.
    """

    def __init__(
        self,
        data_dir: Path,
        embedding_provider: EmbeddingProvider,
        enable_rerank: bool = False,
    ) -> None:
        """Initialize multi-index store.

        Args:
            data_dir: Base data directory for caching
            embedding_provider: Embedding provider for vectorization
            enable_rerank: Whether to enable reranking for searches
        """
        self.data_dir = data_dir
        self.cache_dir = data_dir / "index_cache"
        self.embedding_provider = embedding_provider
        self.enable_rerank = enable_rerank
        self._indexes: dict[IndexKey, LoadedIndex] = {}
        self._temp_dirs: list[Path] = []

    def _get_cache_path(self, url: str) -> Path:
        """Get the cache path for a URL.

        Args:
            url: The URL to get cache path for

        Returns:
            Path to the cached archive
        """
        cache_key = _url_to_cache_key(url)
        return self.cache_dir / f"{cache_key}.tar.gz"

    def _download_archive(self, url: str) -> Path:
        """Download archive from URL or return cached path.

        Args:
            url: URL to download from

        Returns:
            Path to the (cached) archive file
        """
        cache_path = self._get_cache_path(url)

        # Return cached version if exists
        if cache_path.exists():
            console.print(f"  [dim]Using cached: {url}[/dim]")
            return cache_path

        console.print(f"  [blue]Downloading: {url}[/blue]")

        self.cache_dir.mkdir(parents=True, exist_ok=True)

        # Download with progress
        with (
            httpx.Client(timeout=300.0, follow_redirects=True) as client,
            client.stream("GET", url) as response,
        ):
            response.raise_for_status()
            total = int(response.headers.get("content-length", 0))

            with create_download_progress() as progress:
                task = progress.add_task("    Downloading...", total=total)

                # Write to temp file first, then rename
                temp_path = cache_path.with_suffix(".tmp")
                with temp_path.open("wb") as f:
                    for chunk in response.iter_bytes():
                        f.write(chunk)
                        progress.update(task, advance=len(chunk))

                # Rename to final path
                temp_path.rename(cache_path)

        return cache_path

    def _extract_and_load(self, url: str, archive_path: Path) -> LoadedIndex:
        """Extract archive and create VectorStore.

        Args:
            url: Original URL (for reference)
            archive_path: Path to the archive file

        Returns:
            LoadedIndex instance

        Raises:
            ValueError: If archive is invalid
        """
        # Create a persistent temp directory for this index
        temp_dir = Path(tempfile.mkdtemp(prefix="cangjie-index-"))
        self._temp_dirs.append(temp_dir)

        with tarfile.open(archive_path, "r:gz") as tar:
            tar.extractall(temp_dir, filter="data")

        # Read metadata
        metadata_path = temp_dir / ARCHIVE_METADATA_FILE
        if not metadata_path.exists():
            raise ValueError(f"Invalid archive from {url}: missing metadata file")

        metadata = PrebuiltMetadata.model_validate_json(
            metadata_path.read_text(encoding="utf-8")
        )

        # Verify chroma_db exists
        chroma_path = temp_dir / "chroma_db"
        if not chroma_path.exists():
            raise ValueError(f"Invalid archive from {url}: missing chroma_db directory")

        # Create IndexKey from metadata
        key = IndexKey(version=metadata.version, lang=metadata.lang)

        console.print(
            f"  [green]Loaded: {key} (embedding: {metadata.embedding_model})[/green]"
        )

        # Create VectorStore
        reranker = get_reranker_provider() if self.enable_rerank else None
        store = VectorStore(
            db_path=chroma_path,
            embedding_provider=self.embedding_provider,
            reranker=reranker,
        )

        # Verify the index is valid
        if not store.is_indexed():
            raise ValueError(f"Index from {url} is empty or invalid")

        return LoadedIndex(
            url=url,
            metadata=metadata,
            store=store,
            key=key,
        )

    def load_from_url(self, url: str) -> LoadedIndex:
        """Load an index from a URL.

        Downloads the archive (or uses cache), extracts it, and creates
        a VectorStore. The version and language are derived from the
        archive's metadata.

        Args:
            url: URL to the prebuilt index archive

        Returns:
            LoadedIndex with metadata and VectorStore

        Raises:
            httpx.HTTPError: If download fails
            ValueError: If archive is invalid
        """
        archive_path = self._download_archive(url)
        loaded = self._extract_and_load(url, archive_path)

        # Check for duplicate version:lang
        if loaded.key in self._indexes:
            existing = self._indexes[loaded.key]
            console.print(
                f"  [yellow]Warning: {loaded.key} already loaded from {existing.url}, "
                f"replacing with {url}[/yellow]"
            )

        self._indexes[loaded.key] = loaded
        return loaded

    def load_from_urls(self, urls: list[str]) -> dict[IndexKey, LoadedIndex]:
        """Load indexes from multiple URLs.

        Args:
            urls: List of URLs to load

        Returns:
            Dictionary mapping IndexKey to LoadedIndex
        """
        console.print("[blue]Loading indexes from URLs...[/blue]")

        for url in urls:
            try:
                self.load_from_url(url)
            except Exception as e:
                console.print(f"  [red]Failed to load {url}: {e}[/red]")
                raise

        console.print(f"[green]Loaded {len(self._indexes)} indexes[/green]")
        return dict(self._indexes)

    def get_store(self, key: IndexKey) -> VectorStore | None:
        """Get a loaded VectorStore by key.

        Args:
            key: Index identifier

        Returns:
            VectorStore if loaded, None otherwise
        """
        loaded = self._indexes.get(key)
        return loaded.store if loaded else None

    def get_loaded_index(self, key: IndexKey) -> LoadedIndex | None:
        """Get a loaded index by key.

        Args:
            key: Index identifier

        Returns:
            LoadedIndex if loaded, None otherwise
        """
        return self._indexes.get(key)

    def list_loaded(self) -> list[IndexKey]:
        """List all currently loaded indexes.

        Returns:
            List of loaded index keys
        """
        return list(self._indexes.keys())

    def get_all_loaded(self) -> dict[IndexKey, LoadedIndex]:
        """Get all loaded indexes.

        Returns:
            Dictionary of all loaded indexes
        """
        return dict(self._indexes)

    def cleanup(self) -> None:
        """Clean up temporary directories."""
        for temp_dir in self._temp_dirs:
            if temp_dir.exists():
                shutil.rmtree(temp_dir, ignore_errors=True)
        self._temp_dirs.clear()
        self._indexes.clear()

    def clear_cache(self) -> None:
        """Clear the download cache."""
        if self.cache_dir.exists():
            shutil.rmtree(self.cache_dir)
            console.print("[green]Cache cleared.[/green]")

    def __del__(self) -> None:
        """Cleanup on deletion."""
        self.cleanup()


def create_multi_index_store(settings: Settings) -> MultiIndexStore:
    """Factory function to create MultiIndexStore from settings.

    Args:
        settings: Application settings

    Returns:
        Configured MultiIndexStore instance
    """
    embedding_provider = create_embedding_provider(settings)
    return MultiIndexStore(
        data_dir=settings.data_dir,
        embedding_provider=embedding_provider,
        enable_rerank=settings.rerank_type != "none",
    )


def parse_index_urls(urls_str: str) -> list[str]:
    """Parse comma-separated URL list.

    Args:
        urls_str: Comma-separated URLs

    Returns:
        List of URLs
    """
    if not urls_str or not urls_str.strip():
        return []
    return [url.strip() for url in urls_str.split(",") if url.strip()]
