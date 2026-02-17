"""Document source abstraction for reading documentation.

Provides a unified interface for reading documentation from different sources:
- GitDocumentSource: Reads files directly from git repository using GitPython
- RemoteDocumentSource: Reads files from a remote cangjie-mcp HTTP server
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from typing import TYPE_CHECKING, cast

from cangjie_mcp.indexer.loader import (
    extract_code_blocks,
    extract_title_from_content,
)
from cangjie_mcp.utils import logger

if TYPE_CHECKING:
    from git import Repo
    from git.objects import Blob, Tree
    from git.objects.base import IndexObjUnion
    from llama_index.core import Document


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
    def get_document_by_topic(self, topic: str, category: str | None = None) -> Document | None:
        """Get a document by its topic name.

        Args:
            topic: Topic name (file stem without .md extension)
            category: Optional category to narrow search

        Returns:
            Document or None if not found
        """
        ...

    @abstractmethod
    def load_all_documents(self) -> list[Document]:
        """Load all documents from the source.

        Returns:
            List of LlamaIndex Document objects
        """
        ...


class GitDocumentSource(DocumentSource):
    """Reads files directly from git repository using GitPython.

    Uses git tree/blob API to read files without requiring checkout.
    This allows reading from specific versions/tags directly.
    """

    def __init__(self, repo: Repo, version: str, lang: str) -> None:
        """Initialize git document source.

        Args:
            repo: GitPython Repo instance
            version: Git reference (tag, branch, or commit)
            lang: Documentation language ('zh' or 'en')

        Raises:
            ValueError: If the specified version cannot be found in the repository
        """
        self.repo = repo
        self.version = version
        self.lang = lang
        self._lang_dir = "source_zh_cn" if lang == "zh" else "source_en"

        # Cache the tree at the specified version
        try:
            self._commit = repo.commit(version)
            self._tree = self._commit.tree
        except Exception as e:
            raise ValueError(
                f"Git version '{version}' not found in repository. Please ensure the version/tag exists. Error: {e}"
            ) from e

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

    def _create_document(self, content: str, relative_path: str, category: str, topic: str) -> Document:
        """Create a LlamaIndex Document from content.

        Args:
            content: File content
            relative_path: Relative path from docs root
            category: Document category
            topic: Topic name

        Returns:
            LlamaIndex Document
        """
        from llama_index.core import Document

        title = extract_title_from_content(content)
        code_blocks = extract_code_blocks(content)

        return Document(
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

    def get_document_by_topic(self, topic: str, category: str | None = None) -> Document | None:
        """Get a document by its topic name."""
        docs_tree = self._get_docs_tree()
        if docs_tree is None:
            return None

        # Determine search scope
        search_trees: list[tuple[str, IndexObjUnion]] = []
        if category:
            try:
                cat_obj: IndexObjUnion = docs_tree / category
                search_trees = [(category, cat_obj)]
            except KeyError:
                return None
        else:
            search_trees = [(item.name, item) for item in docs_tree if item.type == "tree"]

        # Search for the topic
        filename = f"{topic}.md"
        for cat_name, cat_tree in search_trees:
            if cat_tree.type != "tree":
                continue
            result = self._find_file_in_tree(cat_tree, filename, cat_name)
            if result:
                blob, relative_path = result
                content = self._read_blob_content(blob)
                return self._create_document(content, relative_path, cat_name, topic)

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

    def load_all_documents(self) -> list[Document]:
        """Load all documents from the git repository."""
        docs_tree = self._get_docs_tree()
        if docs_tree is None:
            return []

        documents: list[Document] = []
        for category_item in docs_tree:
            if category_item.type == "tree" and not category_item.name.startswith((".", "_")):
                category = category_item.name
                self._load_docs_from_tree(category_item, category, category, documents)

        logger.info("Loaded %d documents from git.", len(documents))
        return documents

    def _load_docs_from_tree(self, tree: Tree, category: str, prefix: str, documents: list[Document]) -> None:
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
    """

    def __init__(self, server_url: str) -> None:
        """Initialize remote document source.

        Args:
            server_url: Base URL of the remote cangjie-mcp server
        """
        import httpx

        self._server_url = server_url.rstrip("/")
        self._client = httpx.Client(base_url=self._server_url, timeout=30.0)
        self._categories_cache: dict[str, list[str]] | None = None

    def _fetch_topics(self) -> dict[str, list[str]]:
        """Fetch and cache the full topics listing from the server."""
        if self._categories_cache is not None:
            return self._categories_cache

        resp = self._client.get("/topics")
        resp.raise_for_status()
        data = resp.json()
        categories: dict[str, list[str]] = data.get("categories", {})
        self._categories_cache = categories
        return categories

    def is_available(self) -> bool:
        """Check if the remote server is reachable."""
        try:
            resp = self._client.get("/health")
            return resp.status_code == 200
        except Exception:
            return False

    def get_categories(self) -> list[str]:
        """Get list of available categories from the server."""
        topics = self._fetch_topics()
        return sorted(topics.keys())

    def get_topics_in_category(self, category: str) -> list[str]:
        """Get list of topics in a category from the server."""
        topics = self._fetch_topics()
        return sorted(topics.get(category, []))

    def get_document_by_topic(self, topic: str, category: str | None = None) -> Document | None:
        """Get a document by its topic name from the server."""
        from llama_index.core import Document

        # If no category given, find it from the topics listing
        if category is None:
            topics = self._fetch_topics()
            for cat, cat_topics in topics.items():
                if topic in cat_topics:
                    category = cat
                    break
            if category is None:
                return None

        resp = self._client.get(f"/topics/{category}/{topic}")
        if resp.status_code == 404:
            return None
        resp.raise_for_status()
        data = resp.json()

        return Document(
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

    def load_all_documents(self) -> list[Document]:
        """Not supported for remote sources (only needed during index building)."""
        raise NotImplementedError(
            "RemoteDocumentSource does not support load_all_documents. Bulk loading is handled by the server."
        )
