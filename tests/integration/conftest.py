"""Pytest configuration and fixtures for integration tests."""

from pathlib import Path

import pytest

from cangjie_mcp.config import Settings
from cangjie_mcp.indexer.embeddings import get_embedding_provider, reset_embedding_provider
from cangjie_mcp.indexer.loader import DocumentLoader
from cangjie_mcp.indexer.store import VectorStore
from tests.constants import CANGJIE_DOCS_VERSION, CANGJIE_LOCAL_MODEL


def has_openai_credentials() -> bool:
    """Check if OpenAI credentials are available."""
    settings = Settings()
    return bool(
        settings.openai_api_key and settings.openai_api_key != "your-openai-api-key-here"
    )


@pytest.fixture
def integration_docs_dir(temp_data_dir: Path) -> Path:
    """Create a comprehensive documentation directory for integration tests."""
    docs_dir = temp_data_dir / "docs_repo" / "docs" / "dev-guide" / "source_zh_cn"
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


@pytest.fixture
def local_settings(temp_data_dir: Path) -> Settings:
    """Create settings for local embedding integration tests."""
    return Settings(
        docs_version=CANGJIE_DOCS_VERSION,
        docs_lang="zh",
        embedding_type="local",
        local_model=CANGJIE_LOCAL_MODEL,
        data_dir=temp_data_dir,
    )


@pytest.fixture
def openai_settings(temp_data_dir: Path) -> Settings:
    """Create settings for OpenAI embedding integration tests."""
    return Settings(
        docs_version=CANGJIE_DOCS_VERSION,
        docs_lang="zh",
        embedding_type="openai",
        data_dir=temp_data_dir,
    )


@pytest.fixture
def local_indexed_store(
    integration_docs_dir: Path,
    local_settings: Settings,
) -> VectorStore:
    """Create and populate a VectorStore with local embeddings for testing."""
    reset_embedding_provider()
    embedding_provider = get_embedding_provider(local_settings)
    store = VectorStore(
        db_path=local_settings.chroma_db_dir,
        embedding_provider=embedding_provider,
    )

    loader = DocumentLoader(integration_docs_dir)
    documents = loader.load_all_documents()

    store.index_documents(documents)
    store.save_metadata(
        version=local_settings.docs_version,
        lang=local_settings.docs_lang,
        embedding_model=CANGJIE_LOCAL_MODEL,
    )

    return store
