"""Pytest configuration and fixtures."""

import tempfile
from collections.abc import Generator
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from dotenv import load_dotenv

from cangjie_mcp.config import Settings, reset_settings
from cangjie_mcp.indexer.embeddings import reset_embedding_provider
from cangjie_mcp.indexer.reranker import reset_reranker_provider

if TYPE_CHECKING:
    from _pytest.config import Config
    from _pytest.nodes import Item

# Load environment variables from .env file
load_dotenv()


def pytest_configure(config: "Config") -> None:
    """Configure pytest with custom markers."""
    config.addinivalue_line("markers", "unit: Unit tests (fast, no external dependencies)")
    config.addinivalue_line("markers", "integration: Integration tests (may require credentials or external services)")
    config.addinivalue_line(
        "markers", "credentials: Tests that require credentials (skipped without valid credentials)"
    )


def pytest_collection_modifyitems(items: list["Item"]) -> None:
    """Automatically mark tests based on their location."""
    for item in items:
        # Mark tests in tests/integration/ directory as integration tests
        if "integration" in str(item.fspath):
            item.add_marker(pytest.mark.integration)
        # Mark tests in tests/unit/ directory as unit tests
        elif "unit" in str(item.fspath):
            item.add_marker(pytest.mark.unit)


def make_test_settings(**kwargs: object) -> Settings:
    """Create Settings with test defaults.

    Provides all required fields with sensible test defaults.
    Any field can be overridden via kwargs.
    """
    defaults: dict[str, object] = {
        "docs_version": "latest",
        "docs_lang": "zh",
        "embedding_type": "local",
        "local_model": "paraphrase-multilingual-MiniLM-L12-v2",
        "rerank_type": "none",
        "rerank_model": "BAAI/bge-reranker-v2-m3",
        "rerank_top_k": 5,
        "rerank_initial_k": 20,
        "chunk_max_size": 6000,
        "data_dir": Path.home() / ".cangjie-mcp-test",
    }
    defaults.update(kwargs)
    return Settings(**defaults)  # type: ignore[arg-type]


@pytest.fixture
def create_test_settings() -> type[Settings]:
    """Fixture that provides the make_test_settings factory function."""
    return make_test_settings  # type: ignore[return-value]


@pytest.fixture(scope="session")
def has_openai_credentials() -> bool:
    """Check if OpenAI credentials are available."""
    import os

    api_key = os.environ.get("OPENAI_API_KEY")
    return bool(api_key and api_key != "your-openai-api-key-here")


@pytest.fixture
def skip_without_openai_credentials(has_openai_credentials: bool) -> None:
    """Skip test if OpenAI credentials are not available."""
    if not has_openai_credentials:
        pytest.skip("OpenAI credentials not configured (set OPENAI_API_KEY in .env)")


@pytest.fixture
def temp_data_dir() -> Generator[Path]:
    """Create a temporary data directory for tests.

    Uses ignore_cleanup_errors=True to handle Windows issues where
    ChromaDB may keep file handles open during cleanup.
    """
    with tempfile.TemporaryDirectory(ignore_cleanup_errors=True) as temp_dir:
        yield Path(temp_dir)


@pytest.fixture
def test_settings(temp_data_dir: Path) -> Settings:
    """Create test settings with temporary data directory."""
    return make_test_settings(data_dir=temp_data_dir)


@pytest.fixture
def sample_markdown_content() -> str:
    """Sample markdown content for testing."""
    return """# Sample Topic

This is a sample document for testing.

## Code Example

```cangjie
func main() {
    println("Hello, Cangjie!")
}
```

## Another Section

More content here with `inline code`.

```bash
cjc build main.cj
```
"""


@pytest.fixture
def sample_docs_dir(temp_data_dir: Path, sample_markdown_content: str) -> Path:
    """Create a sample documentation directory structure."""
    docs_dir = temp_data_dir / "docs_repo" / "docs" / "dev-guide" / "source_zh_cn"
    docs_dir.mkdir(parents=True)

    # Create some sample files
    (docs_dir / "basics").mkdir()
    (docs_dir / "basics" / "hello_world.md").write_text(sample_markdown_content, encoding="utf-8")

    (docs_dir / "tools").mkdir()
    (docs_dir / "tools" / "cjc.md").write_text(
        """# CJC Compiler

The Cangjie compiler.

## Usage

```bash
cjc [options] <files>
```

## Options

- `-o`: Output file
- `-O`: Optimization level
""",
        encoding="utf-8",
    )

    return docs_dir


@pytest.fixture(autouse=True)
def auto_reset_providers(request: pytest.FixtureRequest) -> Generator[None]:
    """Automatically reset singleton providers after each test.

    Only resets in unit tests to avoid destroying session-scoped
    providers shared by integration tests.
    """
    yield
    # Only reset in unit tests to preserve session-scoped providers
    if "unit" in str(request.path):
        reset_embedding_provider()
        reset_reranker_provider()
        reset_settings()


@pytest.fixture
def reset_providers() -> None:
    """Explicitly reset all singleton providers before a test.

    Use this fixture when you need to ensure providers are reset
    BEFORE a test runs (in addition to after).
    """
    reset_embedding_provider()
    reset_reranker_provider()
    reset_settings()
