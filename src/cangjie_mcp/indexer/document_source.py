"""Document source abstraction for reading documentation.

Provides a unified interface for reading documentation from different sources:
- GitDocumentSource: Reads files directly from git repository using GitPython
- RemoteDocumentSource: Reads files from a remote cangjie-mcp HTTP server
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any, cast

from cangjie_mcp.indexer.loader import (
    extract_code_blocks,
    extract_title_from_content,
)
from cangjie_mcp.utils import logger

if TYPE_CHECKING:
    from git import Repo
    from git.objects import Blob, Tree
    from git.objects.base import IndexObjUnion


@dataclass
class DocData:
    """Lightweight document container for the DocumentSource interface.

    Decouples the DocumentSource abstraction from ``llama_index``, which is
    only needed during index building (via ``DocumentLoader`` / chunker).
    Consumers (MCP tools, HTTP server) only access ``.text`` and
    ``.metadata.get()``.
    """

    text: str
    metadata: dict[str, Any] = field(default_factory=dict[str, Any])
    doc_id: str = ""


class DocumentSource(ABC):
    """Abstract base class for document source providers.

    Provides a unified interface for reading documentation from different sources,
    allowing tools to work with either git repositories or remote servers.
    """

    @abstractmethod
    def is_available(self) -> bool:
        """Check if the document source is available.

        Returns:
            True if the source is ready to read documents
        """
        ...

    @abstractmethod
    def get_categories(self) -> list[str]:
        """Get list of available categories.

        Returns:
            List of category names (directory names)
        """
        ...

    @abstractmethod
    def get_topics_in_category(self, category: str) -> list[str]:
        """Get list of topics in a category.

        Args:
            category: Category name

        Returns:
            List of topic names (file stems without .md extension)
        """
        ...

    @abstractmethod
    def get_document_by_topic(self, topic: str, category: str | None = None) -> DocData | None:
        """Get a document by its topic name.

        Args:
            topic: Topic name (file stem without .md extension)
            category: Optional category to narrow search

        Returns:
            DocData or None if not found
        """
        ...

    @abstractmethod
    def load_all_documents(self) -> list[DocData]:
        """Load all documents from the source.

        Returns:
            List of DocData objects
        """
        ...

    def get_all_topic_names(self) -> list[str]:
        """Get all topic names across all categories.

        Default implementation traverses all categories. Subclasses may
        override for more efficient implementations.

        Returns:
            Sorted list of all topic names
        """
        topics: set[str] = set()
        for cat in self.get_categories():
            topics.update(self.get_topics_in_category(cat))
        return sorted(topics)

    def get_topic_titles(self, category: str) -> dict[str, str]:
        """Get mapping of topic names to their titles for a category.

        Default implementation loads each document to extract its title.
        Subclasses should override for better performance.

        Args:
            category: Category name

        Returns:
            Dict mapping topic name to title string
        """
        titles: dict[str, str] = {}
        for t in self.get_topics_in_category(category):
            doc = self.get_document_by_topic(t, category)
            if doc:
                titles[t] = str(doc.metadata.get("title", ""))
        return titles


class GitDocumentSource(DocumentSource):
    """Reads files directly from git repository using GitPython.

    Uses git tree/blob API to read files without requiring checkout.
    This allows reading from specific versions/tags directly.
    """

    def __init__(self, repo: Repo, version: str, lang: str) -> None:
        """Initialize git document source.

        The caller must ensure the repository HEAD is already at the
        desired commit (e.g. via ``GitManager.resolve_version``).

        Args:
            repo: GitPython Repo instance with HEAD at the target commit
            version: Display version string (e.g. ``"dev(a1b2c3d)"``)
            lang: Documentation language ('zh' or 'en')
        """
        self.repo = repo
        self.version = version
        self.lang = lang
        self._lang_dir = "source_zh_cn" if lang == "zh" else "source_en"

        self._commit = repo.head.commit
        self._tree = self._commit.tree

        # Lazily built mapping from topic name to category.
        # Avoids repeated full-tree traversal when looking up a topic
        # without a category (e.g. get_document_by_topic("classes")).
        self._topic_to_category: dict[str, str] | None = None

    def is_available(self) -> bool:
        """Check if the git source is available.

        Always returns True since __init__ raises if the version is invalid.
        """
        return True

    def _get_docs_tree(self) -> Tree | None:
        """Get the docs subtree for current language.

        Returns:
            Git Tree object for the docs directory, or None if not found
        """
        try:
            # Navigate through the tree structure
            # Use / operator for path traversal in git trees
            docs_path = f"docs/dev-guide/{self._lang_dir}"
            result: IndexObjUnion = self._tree / docs_path
            if result.type != "tree":
                return None
            return result
        except KeyError:
            return None

    def _read_blob_content(self, blob: Blob) -> str:
        """Read content from a git blob.

        Args:
            blob: Git Blob object

        Returns:
            File content as string
        """
        data = cast(bytes, blob.data_stream.read())
        return data.decode("utf-8")

    def _create_document(self, content: str, relative_path: str, category: str, topic: str) -> DocData:
        """Create a DocData from content.

        Args:
            content: File content
            relative_path: Relative path from docs root
            category: Document category
            topic: Topic name

        Returns:
            DocData instance
        """
        title = extract_title_from_content(content)
        code_blocks = extract_code_blocks(content)

        return DocData(
            text=content,
            metadata={
                "file_path": relative_path,
                "category": category,
                "topic": topic,
                "title": title,
                "code_block_count": len(code_blocks),
                "source": "cangjie_docs",
            },
            doc_id=relative_path,
        )

    def get_categories(self) -> list[str]:
        """Get list of available categories."""
        docs_tree = self._get_docs_tree()
        if docs_tree is None:
            return []

        categories: list[str] = []
        for item in docs_tree:
            # Only include directories (trees), not files
            if item.type == "tree" and not item.name.startswith((".", "_")):
                categories.append(str(item.name))

        return sorted(categories)

    def get_topics_in_category(self, category: str) -> list[str]:
        """Get list of topics in a category."""
        docs_tree = self._get_docs_tree()
        if docs_tree is None:
            return []

        try:
            category_obj: IndexObjUnion = docs_tree / category
        except KeyError:
            return []

        # Only process if it's actually a tree (directory)
        if category_obj.type != "tree":
            return []

        topics: list[str] = []
        self._collect_topics(category_obj, topics)
        return sorted(topics)

    def _collect_topics(self, tree: Tree, topics: list[str], prefix: str = "") -> None:
        """Recursively collect topic names from a git tree.

        Args:
            tree: Git Tree object
            topics: List to append topic names to
            prefix: Current path prefix for nested directories
        """
        for item in tree:
            if item.type == "blob" and item.name.endswith(".md"):
                # Get topic name (file stem)
                topic = item.name[:-3]  # Remove .md extension
                topics.append(topic)
            elif item.type == "tree":
                # Recurse into subdirectories
                self._collect_topics(item, topics, f"{prefix}{item.name}/")

    def _build_topic_index(self) -> dict[str, str]:
        """Build a mapping from topic name to category.

        Traverses the git tree once and caches the result so that
        subsequent ``get_document_by_topic`` calls without a category
        can resolve the category in O(1) instead of doing a full
        recursive search.
        """
        if self._topic_to_category is not None:
            return self._topic_to_category

        mapping: dict[str, str] = {}
        docs_tree = self._get_docs_tree()
        if docs_tree is not None:
            for item in docs_tree:
                if item.type == "tree" and not item.name.startswith((".", "_")):
                    for t in self.get_topics_in_category(item.name):
                        mapping.setdefault(t, item.name)
        self._topic_to_category = mapping
        logger.info("Topic index built: %d topics across categories", len(mapping))
        return mapping

    def get_all_topic_names(self) -> list[str]:
        """Get all topic names using the cached topic index."""
        return sorted(self._build_topic_index().keys())

    def get_topic_titles(self, category: str) -> dict[str, str]:
        """Get topic titles by reading the first H1 from each blob.

        Results are cached per category to avoid repeated git tree traversal.
        """
        if not hasattr(self, "_category_titles"):
            self._category_titles: dict[str, dict[str, str]] = {}

        if category in self._category_titles:
            return self._category_titles[category]

        titles: dict[str, str] = {}
        docs_tree = self._get_docs_tree()
        if docs_tree is not None:
            try:
                cat_obj: IndexObjUnion = docs_tree / category
            except KeyError:
                self._category_titles[category] = titles
                return titles
            if cat_obj.type == "tree":
                self._extract_titles_from_tree(cat_obj, titles)

        self._category_titles[category] = titles
        return titles

    def _extract_titles_from_tree(self, tree: Tree, titles: dict[str, str]) -> None:
        """Recursively extract titles from markdown blobs in a git tree."""
        for item in tree:
            if item.type == "blob" and item.name.endswith(".md"):
                topic = item.name[:-3]
                try:
                    content = self._read_blob_content(item)
                    titles[topic] = extract_title_from_content(content)
                except Exception:
                    titles[topic] = ""
            elif item.type == "tree":
                self._extract_titles_from_tree(item, titles)

    def get_document_by_topic(self, topic: str, category: str | None = None) -> DocData | None:
        """Get a document by its topic name."""
        docs_tree = self._get_docs_tree()
        if docs_tree is None:
            return None

        # Resolve category from cached index when not provided
        if not category:
            category = self._build_topic_index().get(topic)
            if category is None:
                return None

        # Targeted search within the known category
        try:
            cat_obj: IndexObjUnion = docs_tree / category
        except KeyError:
            return None
        if cat_obj.type != "tree":
            return None

        filename = f"{topic}.md"
        result = self._find_file_in_tree(cat_obj, filename, category)
        if result:
            blob, relative_path = result
            content = self._read_blob_content(blob)
            return self._create_document(content, relative_path, category, topic)

        return None

    def _find_file_in_tree(self, tree: Tree, filename: str, prefix: str) -> tuple[Blob, str] | None:
        """Recursively find a file in a git tree.

        Args:
            tree: Git Tree object
            filename: File name to find
            prefix: Current path prefix

        Returns:
            Tuple of (blob, relative_path) or None if not found
        """
        try:
            for item in tree:
                if item.type == "blob" and item.name == filename:
                    return (item, f"{prefix}/{item.name}")
                elif item.type == "tree":
                    result = self._find_file_in_tree(item, filename, f"{prefix}/{item.name}")
                    if result:
                        return result
        except (TypeError, AttributeError):
            pass
        return None

    def load_all_documents(self) -> list[DocData]:
        """Load all documents from the git repository."""
        docs_tree = self._get_docs_tree()
        if docs_tree is None:
            return []

        documents: list[DocData] = []
        for category_item in docs_tree:
            if category_item.type == "tree" and not category_item.name.startswith((".", "_")):
                category = category_item.name
                self._load_docs_from_tree(category_item, category, category, documents)

        logger.info("Loaded %d documents from git.", len(documents))
        return documents

    def _load_docs_from_tree(self, tree: Tree, category: str, prefix: str, documents: list[DocData]) -> None:
        """Recursively load documents from a git tree.

        Args:
            tree: Git Tree object
            category: Document category
            prefix: Current path prefix
            documents: List to append documents to
        """
        for item in tree:
            if item.type == "blob" and item.name.endswith(".md"):
                try:
                    content = self._read_blob_content(item)
                    if content.strip():
                        topic = item.name[:-3]  # Remove .md extension
                        relative_path = f"{prefix}/{item.name}"
                        doc = self._create_document(content, relative_path, category, topic)
                        documents.append(doc)
                except Exception as e:
                    logger.warning("Failed to load %s/%s: %s", prefix, item.name, e)
            elif item.type == "tree":
                self._load_docs_from_tree(item, category, f"{prefix}/{item.name}", documents)


class RemoteDocumentSource(DocumentSource):
    """Reads documentation from a remote cangjie-mcp HTTP server.

    Uses httpx to call the server's /topics and /topics/{category}/{topic}
    endpoints. Only supports browsing operations — load_all_documents()
    raises NotImplementedError since bulk loading is only needed during
    index building which happens on the server side.

    NOTE: This class intentionally does **not** import ``llama_index``.
    In remote mode the package is never loaded during initialization,
    and a cold import from an async handler would deadlock the event
    loop on Windows/IocpProactor.
    """

    def __init__(self, server_url: str) -> None:
        """Initialize remote document source.

        Args:
            server_url: Base URL of the remote cangjie-mcp server
        """
        import httpx

        self._server_url = server_url.rstrip("/")
        self._client = httpx.Client(base_url=self._server_url, timeout=30.0)
        self._raw_cache: dict[str, list[dict[str, str]]] | None = None

    def _fetch_topics(self) -> dict[str, list[dict[str, str]]]:
        """Fetch and cache the full topics listing from the server.

        Returns:
            Dict mapping category names to lists of {name, title} dicts.
        """
        if self._raw_cache is not None:
            return self._raw_cache

        resp = self._client.get("/topics")
        resp.raise_for_status()
        data = resp.json()
        raw: dict[str, list[dict[str, str]]] = {}
        for cat, items in data.get("categories", {}).items():
            if isinstance(items, list) and items and isinstance(items[0], dict):
                raw[cat] = items
            elif isinstance(items, list):
                # Backwards compat: server still sends plain string lists
                raw[cat] = [{"name": name, "title": ""} for name in cast(list[str], items)]
        self._raw_cache = raw
        return raw

    def is_available(self) -> bool:
        """Check if the remote server is reachable."""
        try:
            resp = self._client.get("/health")
            return resp.status_code == 200
        except Exception:
            return False

    def get_categories(self) -> list[str]:
        """Get list of available categories from the server."""
        raw = self._fetch_topics()
        return sorted(raw.keys())

    def get_topics_in_category(self, category: str) -> list[str]:
        """Get list of topics in a category from the server."""
        raw = self._fetch_topics()
        items = raw.get(category, [])
        return sorted(item["name"] for item in items)

    def get_topic_titles(self, category: str) -> dict[str, str]:
        """Get topic titles from the cached server response."""
        raw = self._fetch_topics()
        items = raw.get(category, [])
        return {item["name"]: item.get("title", "") for item in items}

    def get_document_by_topic(self, topic: str, category: str | None = None) -> DocData | None:
        """Get a document by its topic name from the server."""
        # If no category given, find it from the topics listing
        if category is None:
            raw = self._fetch_topics()
            for cat, items in raw.items():
                if any(item["name"] == topic for item in items):
                    category = cat
                    break
            if category is None:
                return None

        resp = self._client.get(f"/topics/{category}/{topic}")
        if resp.status_code == 404:
            return None
        resp.raise_for_status()
        data = resp.json()

        return DocData(
            text=data.get("content", ""),
            metadata={
                "file_path": data.get("file_path", ""),
                "category": data.get("category", category),
                "topic": data.get("topic", topic),
                "title": data.get("title", ""),
                "source": "cangjie_docs",
            },
            doc_id=data.get("file_path", f"{category}/{topic}"),
        )

    def load_all_documents(self) -> list[DocData]:
        """Not supported for remote sources (only needed during index building)."""
        raise NotImplementedError(
            "RemoteDocumentSource does not support load_all_documents. Bulk loading is handled by the server."
        )
