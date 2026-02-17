"""Markdown document loader for Cangjie documentation."""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING

from cangjie_mcp.indexer.api_extractor import extract_stdlib_info
from cangjie_mcp.utils import logger

if TYPE_CHECKING:
    from llama_index.core import Document


@dataclass
class CodeBlock:
    """Represents a code block extracted from documentation."""

    language: str
    code: str
    context: str  # Surrounding text/heading


@dataclass
class DocMetadata:
    """Metadata extracted from document path and content."""

    file_path: str
    category: str
    topic: str
    title: str = ""
    code_blocks: list[CodeBlock] = field(default_factory=lambda: [])


def extract_title_from_content(content: str) -> str:
    """Extract title from markdown content.

    Args:
        content: Markdown content

    Returns:
        Title string or empty string if not found
    """
    # Look for first H1 heading
    match = re.search(r"^#\s+(.+)$", content, re.MULTILINE)
    if match:
        return match.group(1).strip()
    return ""


def extract_code_blocks(content: str) -> list[CodeBlock]:
    """Extract code blocks from markdown content.

    Args:
        content: Markdown content

    Returns:
        List of CodeBlock objects
    """
    code_blocks: list[CodeBlock] = []
    # Pattern to match fenced code blocks with optional language
    pattern = r"```(\w*)\n(.*?)```"
    matches = re.finditer(pattern, content, re.DOTALL)

    for match in matches:
        language = match.group(1) or "text"
        code = match.group(2).strip()

        # Get surrounding context (previous heading or paragraph)
        start_pos = match.start()
        preceding_text = content[:start_pos]

        # Find the last heading before this code block
        heading_match = re.search(r"(^#{1,6}\s+.+$)", preceding_text, re.MULTILINE)
        context = heading_match.group(1) if heading_match else ""

        code_blocks.append(CodeBlock(language=language, code=code, context=context))

    return code_blocks


def extract_metadata_from_path(file_path: Path, source_dir: Path) -> DocMetadata:
    """Extract metadata from file path.

    Args:
        file_path: Path to the markdown file
        source_dir: Base source directory

    Returns:
        DocMetadata with category and topic
    """
    relative_path = file_path.relative_to(source_dir)
    parts = relative_path.parts

    # Category is typically the first directory
    category = parts[0] if len(parts) > 1 else "general"

    # Topic is the filename without extension
    topic = file_path.stem

    return DocMetadata(
        file_path=str(relative_path),
        category=category,
        topic=topic,
    )


class DocumentLoader:
    """Loads and processes Cangjie documentation files."""

    def __init__(self, source_dir: Path) -> None:
        """Initialize document loader.

        Args:
            source_dir: Path to documentation source directory
        """
        self.source_dir = source_dir

    def load_all_documents(self) -> list[Document]:
        """Load all markdown documents from source directory.

        Returns:
            List of LlamaIndex Document objects
        """
        documents: list[Document] = []
        md_files = list(self.source_dir.rglob("*.md"))

        logger.info("Found %d markdown files.", len(md_files))

        for file_path in md_files:
            try:
                doc = self._load_document(file_path)
                if doc:
                    documents.append(doc)
            except Exception as e:
                logger.warning("Failed to load %s: %s", file_path, e)

        logger.info("Loaded %d documents.", len(documents))
        return documents

    def _load_document(self, file_path: Path) -> Document | None:
        """Load a single document.

        Args:
            file_path: Path to markdown file

        Returns:
            LlamaIndex Document or None if file is empty
        """
        from llama_index.core import Document

        content = file_path.read_text(encoding="utf-8")
        if not content.strip():
            return None

        # Extract metadata
        metadata = extract_metadata_from_path(file_path, self.source_dir)
        metadata.title = extract_title_from_content(content)
        metadata.code_blocks = extract_code_blocks(content)

        # Extract stdlib-specific metadata dynamically
        stdlib_info = extract_stdlib_info(content)

        # Create LlamaIndex document
        # Note: ChromaDB metadata only supports scalar types (str, int, float, None)
        # so we serialize lists to JSON strings
        return Document(
            text=content,
            metadata={
                "file_path": metadata.file_path,
                "category": metadata.category,
                "topic": metadata.topic,
                "title": metadata.title,
                "code_block_count": len(metadata.code_blocks),
                "source": "cangjie_docs",
                # Stdlib metadata (lists serialized as JSON for ChromaDB compatibility)
                "is_stdlib": stdlib_info.is_stdlib,
                "packages": json.dumps(stdlib_info.packages),
                "type_names": json.dumps(stdlib_info.type_names),
            },
            doc_id=metadata.file_path,
        )

    def load_documents_by_category(self, category: str) -> list[Document]:
        """Load documents from a specific category.

        Args:
            category: Category name (directory name)

        Returns:
            List of documents in that category
        """
        category_dir = self.source_dir / category
        if not category_dir.exists():
            return []

        documents: list[Document] = []
        for file_path in category_dir.rglob("*.md"):
            try:
                doc = self._load_document(file_path)
                if doc:
                    documents.append(doc)
            except Exception as e:
                logger.warning("Failed to load %s: %s", file_path, e)

        return documents

    def get_categories(self) -> list[str]:
        """Get list of available categories.

        Returns:
            List of category names
        """
        return sorted(
            item.name for item in self.source_dir.iterdir() if item.is_dir() and not item.name.startswith((".", "_"))
        )

    def get_topics_in_category(self, category: str) -> list[str]:
        """Get list of topics in a category.

        Args:
            category: Category name

        Returns:
            List of topic names
        """
        category_dir = self.source_dir / category
        if not category_dir.exists():
            return []

        return sorted(file_path.stem for file_path in category_dir.rglob("*.md"))

    def get_document_by_topic(self, topic: str, category: str | None = None) -> Document | None:
        """Get a document by its topic name.

        Args:
            topic: Topic name (file stem)
            category: Optional category to narrow search

        Returns:
            Document or None if not found
        """
        search_dir = self.source_dir / category if category else self.source_dir

        for file_path in search_dir.rglob(f"{topic}.md"):
            return self._load_document(file_path)

        return None
