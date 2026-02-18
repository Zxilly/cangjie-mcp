"""Pytest configuration and fixtures for integration tests."""

import os
import socket
import tempfile
import threading
import time
from collections.abc import Generator
from pathlib import Path

import pytest
import uvicorn

from cangjie_mcp.config import IndexInfo, Settings
from cangjie_mcp.indexer.document_source import DocData, DocumentSource
from cangjie_mcp.indexer.embeddings import get_embedding_provider
from cangjie_mcp.indexer.loader import (
    DocumentLoader,
    extract_code_blocks,
    extract_metadata_from_path,
    extract_title_from_content,
)
from cangjie_mcp.indexer.reranker import LocalReranker
from cangjie_mcp.indexer.search_index import SearchIndex
from cangjie_mcp.indexer.store import SearchResult, VectorStore
from tests.constants import CANGJIE_DOCS_VERSION, CANGJIE_LOCAL_MODEL, CANGJIE_RERANKER_MODEL

# Read embedding configuration from environment (falls back to local defaults)
_EMBEDDING_TYPE: str = os.environ.get("CANGJIE_EMBEDDING_TYPE", "local")
_OPENAI_API_KEY: str | None = os.environ.get("OPENAI_API_KEY")
_OPENAI_BASE_URL: str = os.environ.get("OPENAI_BASE_URL", "https://api.openai.com/v1")
_OPENAI_MODEL: str = os.environ.get("OPENAI_EMBEDDING_MODEL", "text-embedding-3-small")


def _embedding_model_name() -> str:
    """Return the canonical embedding model name for the current config."""
    if _EMBEDDING_TYPE == "openai":
        return f"openai:{_OPENAI_MODEL}"
    return f"local:{CANGJIE_LOCAL_MODEL}"


def _make_settings(data_dir: Path, **overrides: object) -> Settings:
    """Create Settings using environment-driven embedding config."""
    defaults: dict[str, object] = {
        "docs_version": CANGJIE_DOCS_VERSION,
        "docs_lang": "zh",
        "embedding_type": _EMBEDDING_TYPE,
        "local_model": CANGJIE_LOCAL_MODEL,
        "openai_api_key": _OPENAI_API_KEY,
        "openai_base_url": _OPENAI_BASE_URL,
        "openai_model": _OPENAI_MODEL,
        "rerank_type": "none",
        "rerank_model": "BAAI/bge-reranker-v2-m3",
        "rerank_top_k": 5,
        "rerank_initial_k": 20,
        "chunk_max_size": 6000,
        "data_dir": data_dir,
    }
    defaults.update(overrides)
    return Settings(**defaults)  # type: ignore[arg-type]


class TestDocumentSource(DocumentSource):
    """Configurable document source for testing.

    Reads markdown files from a local directory and serves them via the
    DocumentSource interface. Only used in tests — production code uses
    GitDocumentSource or RemoteDocumentSource.
    """

    __test__ = False

    def __init__(self, docs_dir: Path) -> None:
        self.docs_dir = docs_dir

    def is_available(self) -> bool:
        return self.docs_dir.exists() and self.docs_dir.is_dir()

    def get_categories(self) -> list[str]:
        if not self.is_available():
            return []
        return sorted(
            item.name for item in self.docs_dir.iterdir() if item.is_dir() and not item.name.startswith((".", "_"))
        )

    def get_topics_in_category(self, category: str) -> list[str]:
        category_dir = self.docs_dir / category
        if not category_dir.exists():
            return []
        return sorted(fp.stem for fp in category_dir.rglob("*.md"))

    def get_document_by_topic(self, topic: str, category: str | None = None) -> DocData | None:
        if not self.is_available():
            return None
        search_dir = self.docs_dir / category if category else self.docs_dir
        for file_path in search_dir.rglob(f"{topic}.md"):
            return self._load_document(file_path)
        return None

    def _load_document(self, file_path: Path) -> DocData | None:
        content = file_path.read_text(encoding="utf-8")
        if not content.strip():
            return None

        metadata = extract_metadata_from_path(file_path, self.docs_dir)
        metadata.title = extract_title_from_content(content)
        metadata.code_blocks = extract_code_blocks(content)

        return DocData(
            text=content,
            metadata={
                "file_path": metadata.file_path,
                "category": metadata.category,
                "topic": metadata.topic,
                "title": metadata.title,
                "code_block_count": len(metadata.code_blocks),
                "source": "cangjie_docs",
            },
            doc_id=metadata.file_path,
        )

    def load_all_documents(self) -> list[DocData]:
        if not self.is_available():
            return []
        documents: list[DocData] = []
        for file_path in self.docs_dir.rglob("*.md"):
            doc = self._load_document(file_path)
            if doc:
                documents.append(doc)
        return documents


class VectorStoreSearchIndex(SearchIndex):
    """Test adapter wrapping a VectorStore as a SearchIndex."""

    def __init__(self, store: VectorStore) -> None:
        self._store = store

    def init(self) -> IndexInfo:
        raise NotImplementedError("Test adapter — call query() directly")

    async def query(
        self,
        query: str,
        top_k: int = 5,
        category: str | None = None,
        rerank: bool = True,
    ) -> list[SearchResult]:
        import asyncio

        return await asyncio.to_thread(
            self._store.search,
            query=query,
            top_k=top_k,
            category=category,
            use_rerank=rerank,
        )


def has_openai_credentials() -> bool:
    """Check if OpenAI credentials are available via environment variable."""
    api_key = os.environ.get("OPENAI_API_KEY", "")
    return bool(api_key and api_key != "your-openai-api-key-here")


# ── Session-scoped fixtures (shared by read-only integration tests) ──


@pytest.fixture(scope="session")
def shared_temp_dir() -> Generator[Path]:
    """Session-scoped temporary directory for shared fixtures."""
    with tempfile.TemporaryDirectory(ignore_cleanup_errors=True) as d:
        yield Path(d)


@pytest.fixture(scope="session")
def integration_docs_dir(shared_temp_dir: Path) -> Path:
    """Create a comprehensive documentation directory for integration tests.

    Session-scoped: documents are created once and shared across all tests.
    """
    docs_dir = shared_temp_dir / "docs_repo" / "docs" / "dev-guide" / "source_zh_cn"
    docs_dir.mkdir(parents=True)

    # Create basics category
    basics_dir = docs_dir / "basics"
    basics_dir.mkdir()

    (basics_dir / "hello_world.md").write_text(
        """# Hello World

仓颉语言的第一个程序。

## 代码示例

```cangjie
func main() {
    println("Hello, Cangjie!")
}
```

## 运行方式

使用以下命令编译运行：

```bash
cjc hello.cj -o hello
./hello
```
""",
        encoding="utf-8",
    )

    (basics_dir / "variables.md").write_text(
        """# 变量与类型

仓颉语言支持多种数据类型。

## 变量声明

使用 `let` 声明不可变变量，使用 `var` 声明可变变量。

```cangjie
let x: Int = 10
var y: String = "Hello"
```

## 基本类型

- Int: 整数类型
- Float: 浮点类型
- String: 字符串类型
- Bool: 布尔类型
""",
        encoding="utf-8",
    )

    # Create syntax category
    syntax_dir = docs_dir / "syntax"
    syntax_dir.mkdir()

    (syntax_dir / "functions.md").write_text(
        """# 函数

函数是仓颉语言的基本构建块。

## 函数定义

使用 `func` 关键字定义函数：

```cangjie
func add(a: Int, b: Int): Int {
    return a + b
}

func greet(name: String): Unit {
    println("Hello, ${name}!")
}
```

## 函数调用

```cangjie
let result = add(1, 2)
greet("World")
```
""",
        encoding="utf-8",
    )

    (syntax_dir / "pattern_matching.md").write_text(
        """# 模式匹配

仓颉语言支持强大的模式匹配功能。

## match 表达式

```cangjie
func describe(x: Int): String {
    match x {
        0 => "zero"
        1 => "one"
        _ => "many"
    }
}
```

## 类型模式

```cangjie
func process(value: Any): String {
    match value {
        n: Int => "integer: ${n}"
        s: String => "string: ${s}"
        _ => "unknown"
    }
}
```
""",
        encoding="utf-8",
    )

    # Create tools category
    tools_dir = docs_dir / "tools"
    tools_dir.mkdir()

    (tools_dir / "cjc.md").write_text(
        """# CJC 编译器

cjc 是仓颉语言的编译器。

## 基本用法

```bash
cjc [options] <source_files>
```

## 常用选项

- `-o <file>`: 指定输出文件名
- `-O <level>`: 优化级别 (0-3)
- `--debug`: 启用调试信息

## 示例

编译单个文件：

```bash
cjc main.cj -o main
```

编译多个文件：

```bash
cjc main.cj utils.cj -o app
```
""",
        encoding="utf-8",
    )

    (tools_dir / "cjpm.md").write_text(
        """# CJPM 包管理器

cjpm 是仓颉语言的包管理器。

## 常用命令

### 初始化项目

```bash
cjpm init
```

### 构建项目

```bash
cjpm build
```

### 运行测试

```bash
cjpm test
```

### 添加依赖

```bash
cjpm add <package_name>
```
""",
        encoding="utf-8",
    )

    return docs_dir


@pytest.fixture(scope="session")
def test_doc_source(integration_docs_dir: Path) -> TestDocumentSource:
    """Session-scoped TestDocumentSource built from integration_docs_dir."""
    return TestDocumentSource(integration_docs_dir)


@pytest.fixture(scope="session")
def shared_local_settings(shared_temp_dir: Path) -> Settings:
    """Session-scoped settings for integration tests."""
    return _make_settings(shared_temp_dir)


@pytest.fixture(scope="session")
def shared_embedding_provider(shared_local_settings: Settings):
    """Session-scoped embedding provider (loaded once for the entire session)."""
    return get_embedding_provider(shared_local_settings)


@pytest.fixture(scope="session")
def local_indexed_store(
    integration_docs_dir: Path,
    shared_local_settings: Settings,
    shared_embedding_provider,
) -> VectorStore:
    """Session-scoped indexed VectorStore (created and indexed once).

    Shared by all read-only integration tests. Tests that modify the
    store should create their own function-scoped fixtures instead.
    """
    index_info = IndexInfo.from_settings(shared_local_settings)
    store = VectorStore(
        db_path=index_info.chroma_db_dir,
        embedding_provider=shared_embedding_provider,
    )

    loader = DocumentLoader(integration_docs_dir)
    documents = loader.load_all_documents()

    store.index_documents(documents)
    store.save_metadata(
        version=shared_local_settings.docs_version,
        lang=shared_local_settings.docs_lang,
        embedding_model=_embedding_model_name(),
    )

    return store


@pytest.fixture(scope="session")
def shared_local_reranker() -> LocalReranker:
    """Session-scoped local reranker (loaded once for the entire session).

    Pre-warms the cross-encoder model so the loading cost (~15 s for the
    default model) appears in fixture setup rather than in the first test
    call.  The model is controlled by CANGJIE_TEST_RERANKER_MODEL.
    """
    reranker = LocalReranker(model_name=CANGJIE_RERANKER_MODEL)
    # Trigger model loading now instead of lazily on first rerank()
    reranker._get_reranker(top_n=3)
    return reranker


@pytest.fixture(scope="session")
def shared_indexed_store_with_reranker(
    local_indexed_store: VectorStore,
    shared_embedding_provider,
    shared_local_reranker: LocalReranker,
) -> VectorStore:
    """Session-scoped VectorStore with reranker attached.

    Reuses the same ChromaDB as ``local_indexed_store`` — the reranker
    is a post-retrieval step and doesn't change the indexed data.
    This avoids a redundant re-indexing of all documents.
    """
    store = VectorStore(
        db_path=local_indexed_store.db_path,
        embedding_provider=shared_embedding_provider,
        reranker=shared_local_reranker,
    )
    return store


@pytest.fixture(scope="session")
def shared_small_chunk_settings(shared_temp_dir: Path) -> Settings:
    """Session-scoped settings with small chunk size."""
    return _make_settings(
        shared_temp_dir / "small_chunk",
        chunk_max_size=200,
    )


@pytest.fixture(scope="session")
def shared_small_chunk_store(
    integration_docs_dir: Path,
    shared_small_chunk_settings: Settings,
    shared_embedding_provider,
) -> VectorStore:
    """Session-scoped VectorStore indexed with small chunks (created once)."""
    index_info = IndexInfo.from_settings(shared_small_chunk_settings)
    store = VectorStore(
        db_path=index_info.chroma_db_dir,
        embedding_provider=shared_embedding_provider,
    )

    loader = DocumentLoader(integration_docs_dir)
    documents = loader.load_all_documents()

    store.index_documents(documents)
    store.save_metadata(
        version=shared_small_chunk_settings.docs_version,
        lang=shared_small_chunk_settings.docs_lang,
        embedding_model=_embedding_model_name(),
    )

    return store


# ── Function-scoped fixtures (for tests that need write isolation) ──


@pytest.fixture
def local_settings(temp_data_dir: Path) -> Settings:
    """Function-scoped settings for tests that need write isolation."""
    return _make_settings(temp_data_dir)


@pytest.fixture
def pre_indexed_store(
    temp_data_dir: Path,
    local_indexed_store: VectorStore,
    shared_embedding_provider,
) -> VectorStore:
    """Function-scoped store pre-filled by copying session-scoped ChromaDB.

    Much faster than re-indexing from scratch (~0.05 s vs ~1 s per test)
    because it copies the already-indexed database files instead of
    re-generating embeddings.  Use this for tests that need write
    isolation (clear, reindex, save_metadata) on an already-indexed store.
    """
    import shutil

    dest = temp_data_dir / "chroma_db"
    shutil.copytree(local_indexed_store.db_path, dest)
    return VectorStore(
        db_path=dest,
        embedding_provider=shared_embedding_provider,
    )


@pytest.fixture
def openai_settings(temp_data_dir: Path) -> Settings:
    """Create settings for OpenAI embedding integration tests."""
    return _make_settings(temp_data_dir, embedding_type="openai")


# ── HTTP server fixture (session-scoped, for E2E HTTP tests) ──


def _find_free_port() -> int:
    """Find a free TCP port on localhost."""
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


@pytest.fixture(scope="session")
def http_server_url(
    local_indexed_store: VectorStore,
    test_doc_source: TestDocumentSource,
    shared_local_settings: Settings,
) -> Generator[str]:
    """Start a real uvicorn HTTP server in a background thread and yield its base URL.

    Session-scoped: the server is started once and shared across all
    HTTP integration tests.  Runs in a separate thread with its own
    event loop so it works regardless of the test event loop scope.
    """
    from cangjie_mcp.indexer.store import IndexMetadata
    from cangjie_mcp.server.http import create_http_app

    index_info = IndexInfo.from_settings(shared_local_settings)
    search_index = VectorStoreSearchIndex(local_indexed_store)
    metadata = IndexMetadata(
        version=index_info.version,
        lang=index_info.lang,
        embedding_model=index_info.embedding_model_name,
        document_count=local_indexed_store.collection.count(),
    )

    app = create_http_app(search_index, test_doc_source, metadata)
    port = _find_free_port()
    config = uvicorn.Config(app, host="127.0.0.1", port=port, log_level="warning")
    server = uvicorn.Server(config)

    thread = threading.Thread(target=server.run, daemon=True)
    thread.start()

    # Wait for server to be ready
    for _ in range(200):
        if server.started:
            break
        time.sleep(0.05)

    yield f"http://127.0.0.1:{port}"

    server.should_exit = True
    thread.join(timeout=5)
