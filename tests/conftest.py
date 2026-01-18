"""Pytest configuration and fixtures."""

import tempfile
from collections.abc import Generator
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from dotenv import load_dotenv

from cangjie_mcp.config import OpenAISettings, Settings

if TYPE_CHECKING:
    from _pytest.config import Config
    from _pytest.nodes import Item

# Load environment variables from .env file
load_dotenv()


def pytest_configure(config: "Config") -> None:
    """Configure pytest with custom markers."""
    config.addinivalue_line("markers", "unit: Unit tests (fast, no external dependencies)")
    config.addinivalue_line(
        "markers", "integration: Integration tests (may require credentials or external services)"
    )
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


@pytest.fixture(scope="session")
def has_openai_credentials() -> bool:
    """Check if OpenAI credentials are available."""
    settings = OpenAISettings()
    return bool(settings.api_key and settings.api_key != "your-openai-api-key-here")


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
    return Settings(
        docs_version="latest",
        docs_lang="zh",
        embedding_type="local",
        data_dir=temp_data_dir,
    )


@pytest.fixture
def sample_markdown_content() -> str:
    """Sample markdown content for testing."""
    return '''# Sample Topic

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
'''


@pytest.fixture
def sample_docs_dir(temp_data_dir: Path, sample_markdown_content: str) -> Path:
    """Create a sample documentation directory structure."""
    docs_dir = temp_data_dir / "docs_repo" / "docs" / "dev-guide" / "source_zh_cn"
    docs_dir.mkdir(parents=True)

    # Create some sample files
    (docs_dir / "basics").mkdir()
    (docs_dir / "basics" / "hello_world.md").write_text(
        sample_markdown_content, encoding="utf-8"
    )

    (docs_dir / "tools").mkdir()
    (docs_dir / "tools" / "cjc.md").write_text(
        '''# CJC Compiler

The Cangjie compiler.

## Usage

```bash
cjc [options] <files>
```

## Options

- `-o`: Output file
- `-O`: Optimization level
''',
        encoding="utf-8",
    )

    return docs_dir
