"""Pytest configuration and fixtures."""

import tempfile
from collections.abc import Generator
from pathlib import Path

import pytest

from cangjie_mcp.config import Settings


@pytest.fixture
def temp_data_dir() -> Generator[Path]:
    """Create a temporary data directory for tests."""
    with tempfile.TemporaryDirectory() as temp_dir:
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
