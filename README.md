# Cangjie MCP Server

仓颉编程语言文档的 MCP (Model Context Protocol) 服务器，提供基于 RAG (Retrieval-Augmented Generation) 的智能文档搜索功能。

## 功能特性

- 基于向量检索的语义搜索
- 支持本地嵌入模型和 OpenAI 兼容嵌入模型
- 支持本地 Rerank 和 OpenAI 兼容 Rerank API（SiliconFlow 等）
- 自动下载和索引仓颉官方文档
- 支持预构建索引的创建和下载
- 支持多索引 HTTP 服务器模式
- 完整的 MCP 协议支持

## 安装

```bash
pip install cangjie-mcp

# 或
uv pip install cangjie-mcp
```

## 快速开始

### 推荐配置（使用预构建索引 + 本地 Rerank）

为获得最佳体验，配置预构建索引 URL 并启用本地 rerank：

```bash
export CANGJIE_PREBUILT_URL=https://github.com/Zxilly/cangjie-mcp/releases/download/prebuilt-v1.0.7-zh/cangjie-index-v1.0.7-zh.tar.gz
cangjie-mcp --rerank local
```

或创建 `.env` 文件保存配置（见下方配置文件示例）。

### 其他启动方式

```bash
cangjie-mcp
cangjie-mcp --version v1.0.7 --lang zh
```

## 命令

### 默认命令 - 启动 MCP 服务器 (stdio)

直接运行 `cangjie-mcp` 启动 stdio 模式的 MCP 服务器，适用于 Claude Code 等 MCP 客户端集成。

```bash
cangjie-mcp [OPTIONS]
```

**选项：**

| 选项 | 简写 | 类型 | 默认值 | 说明 |
|------|------|------|--------|------|
| `--version` | `-v` | TEXT | `latest` | 文档版本（git tag） |
| `--lang` | `-l` | TEXT | `zh` | 文档语言（zh/en） |
| `--embedding` | `-e` | TEXT | `local` | 嵌入类型（local/openai） |
| `--local-model` | | TEXT | `paraphrase-multilingual-MiniLM-L12-v2` | 本地 HuggingFace 嵌入模型名称 |
| `--openai-api-key` | | TEXT | | OpenAI 兼容 API 密钥 |
| `--openai-base-url` | | TEXT | `https://api.openai.com/v1` | OpenAI 兼容 API 基础 URL |
| `--openai-model` | | TEXT | `text-embedding-3-small` | OpenAI 嵌入模型 |
| `--rerank` | `-r` | TEXT | `none` | Rerank 类型（none/local/openai） |
| `--rerank-model` | | TEXT | `BAAI/bge-reranker-v2-m3` | Rerank 模型名称 |
| `--rerank-top-k` | | INTEGER | `5` | Rerank 后返回的结果数量 |
| `--rerank-initial-k` | | INTEGER | `20` | Rerank 前检索的候选数量 |
| `--data-dir` | `-d` | PATH | `~/.cangjie-mcp` | 数据目录路径 |

**示例：**

```bash
cangjie-mcp --embedding local --rerank local
cangjie-mcp --embedding openai --openai-api-key sk-xxx
cangjie-mcp --rerank openai --openai-api-key sk-xxx --openai-base-url https://api.siliconflow.cn/v1
```

### `serve` - 启动 HTTP 服务器（多索引模式）

启动支持多索引的 HTTP 服务器，从 URL 加载预构建索引。

```bash
cangjie-mcp serve [OPTIONS]
```

**选项：**

| 选项 | 简写 | 类型 | 默认值 | 说明 |
|------|------|------|--------|------|
| `--indexes` | `-i` | TEXT | | 预构建索引 URL 列表（逗号分隔） |
| `--host` | `-H` | TEXT | `127.0.0.1` | HTTP 服务器主机地址 |
| `--port` | `-p` | INTEGER | `8000` | HTTP 服务器端口 |
| `--embedding` | `-e` | TEXT | `local` | 嵌入类型（local/openai） |
| `--rerank` | `-r` | TEXT | `none` | Rerank 类型（none/local/openai） |
| `--rerank-model` | | TEXT | `BAAI/bge-reranker-v2-m3` | Rerank 模型名称 |

**示例：**

```bash
cangjie-mcp serve --indexes "https://example.com/cangjie-index-v1-zh.tar.gz"
cangjie-mcp serve --indexes "https://example.com/v1-zh.tar.gz,https://example.com/v2-en.tar.gz"
cangjie-mcp serve --indexes "..." --host 0.0.0.0 --port 8080
```

访问方式（路由从索引元数据派生）：
- `POST http://localhost:8000/v1/zh/mcp` → v1 中文文档
- `POST http://localhost:8000/v2/en/mcp` → v2 英文文档

### `prebuilt build` - 构建预构建索引

构建预构建索引归档文件，包含向量数据库和元数据。

```bash
cangjie-mcp prebuilt build [OPTIONS]
```

**选项：**

| 选项 | 简写 | 类型 | 默认值 | 说明 |
|------|------|------|--------|------|
| `--version` | `-v` | TEXT | `latest` | 文档版本（git tag） |
| `--lang` | `-l` | TEXT | `zh` | 文档语言（zh/en） |
| `--embedding` | `-e` | TEXT | `local` | 嵌入类型（local/openai） |
| `--embedding-model` | `-m` | TEXT | | 嵌入模型名称 |
| `--data-dir` | `-d` | PATH | `~/.cangjie-mcp` | 数据目录 |
| `--output` | `-o` | PATH | | 输出目录或文件路径 |

**示例：**

```bash
cangjie-mcp prebuilt build
cangjie-mcp prebuilt build --version v0.53.18 --lang zh
cangjie-mcp prebuilt build --output ./my-index.tar.gz
```

### `prebuilt download` - 下载预构建索引

从指定 URL 下载预构建索引。

```bash
cangjie-mcp prebuilt download [OPTIONS]
```

**选项：**

| 选项 | 简写 | 类型 | 默认值 | 说明 |
|------|------|------|--------|------|
| `--url` | `-u` | TEXT | | 下载 URL |
| `--version` | `-v` | TEXT | `latest` | 要下载的版本 |
| `--lang` | `-l` | TEXT | `zh` | 要下载的语言 |

**示例：**

```bash
cangjie-mcp prebuilt download --url https://github.com/Zxilly/cangjie-mcp/releases/download/prebuilt-v1.0.7-zh/cangjie-index-v1.0.7-zh.tar.gz
```

### `prebuilt list` - 列出预构建索引

列出本地可用的预构建索引。

```bash
cangjie-mcp prebuilt list
```

## 环境变量配置

所有配置都可以通过 CLI 参数或环境变量设置。运行 `cangjie-mcp --help` 查看所有选项及其对应的环境变量。

### 文档配置

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `CANGJIE_DOCS_VERSION` | `latest` | 文档版本（git tag） |
| `CANGJIE_DOCS_LANG` | `zh` | 文档语言（zh/en） |
| `CANGJIE_DATA_DIR` | `~/.cangjie-mcp` | 数据目录路径 |
| `CANGJIE_PREBUILT_URL` | | 预构建索引下载 URL |

### 嵌入配置

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `CANGJIE_EMBEDDING_TYPE` | `local` | 嵌入类型（local/openai） |
| `CANGJIE_LOCAL_MODEL` | `paraphrase-multilingual-MiniLM-L12-v2` | 本地 HuggingFace 嵌入模型 |

### OpenAI 兼容 API 配置

适用于 OpenAI、SiliconFlow 等兼容 API。

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `OPENAI_API_KEY` | | API 密钥 |
| `OPENAI_BASE_URL` | `https://api.openai.com/v1` | API 基础 URL |
| `OPENAI_EMBEDDING_MODEL` | `text-embedding-3-small` | 嵌入模型 |

### Rerank 配置

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `CANGJIE_RERANK_TYPE` | `none` | Rerank 类型（none/local/openai） |
| `CANGJIE_RERANK_MODEL` | `BAAI/bge-reranker-v2-m3` | Rerank 模型名称 |
| `CANGJIE_RERANK_TOP_K` | `5` | Rerank 后返回的结果数量 |
| `CANGJIE_RERANK_INITIAL_K` | `20` | Rerank 前检索的候选数量 |

### HTTP 服务器配置

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `CANGJIE_HTTP_HOST` | `127.0.0.1` | HTTP 服务器主机地址 |
| `CANGJIE_HTTP_PORT` | `8000` | HTTP 服务器端口 |
| `CANGJIE_INDEXES` | | 预构建索引 URL 列表（逗号分隔） |

## 配置文件示例

创建 `.env` 文件：

```env
CANGJIE_DOCS_VERSION=v1.0.7
CANGJIE_DOCS_LANG=zh
CANGJIE_PREBUILT_URL=https://github.com/Zxilly/cangjie-mcp/releases/download/prebuilt-v1.0.7-zh/cangjie-index-v1.0.7-zh.tar.gz
CANGJIE_EMBEDDING_TYPE=local
CANGJIE_RERANK_TYPE=local
```

## MCP 工具

服务器提供以下 MCP 工具：

| 工具名称 | 说明 |
|----------|------|
| `cangjie_search_docs` | 搜索文档，支持语义搜索、分类过滤和分页 |
| `cangjie_get_topic` | 获取指定主题的完整文档内容 |
| `cangjie_list_topics` | 列出所有可用主题，支持按分类筛选 |
| `cangjie_get_code_examples` | 获取代码示例 |
| `cangjie_get_tool_usage` | 获取工具使用说明（如 cjc、cjpm） |


## MCP 客户端配置

以下是在各种 MCP 客户端中配置 Cangjie MCP 的方法。

<details>
<summary>Claude Code</summary>

#### 命令行添加（推荐）

```bash
claude mcp add \
  -e CANGJIE_PREBUILT_URL=https://github.com/Zxilly/cangjie-mcp/releases/download/prebuilt-v1.0.7-zh/cangjie-index-v1.0.7-zh.tar.gz \
  -e CANGJIE_RERANK_TYPE=local \
  cangjie -- uvx cangjie-mcp
```

#### 配置文件

在项目根目录创建 `.mcp.json`：

```json
{
  "mcpServers": {
    "cangjie": {
      "command": "uvx",
      "args": ["cangjie-mcp", "--rerank", "local"],
      "env": {
        "CANGJIE_PREBUILT_URL": "https://github.com/Zxilly/cangjie-mcp/releases/download/prebuilt-v1.0.7-zh/cangjie-index-v1.0.7-zh.tar.gz"
      }
    }
  }
}
```

#### 常用命令

```bash
claude mcp list
claude mcp get cangjie
claude mcp remove cangjie
```

</details>

<details>
<summary>Cursor</summary>

在 Cursor 设置中添加 MCP 服务器，或编辑 `~/.cursor/mcp.json`：

```json
{
  "mcpServers": {
    "cangjie": {
      "command": "uvx",
      "args": ["cangjie-mcp", "--rerank", "local"],
      "env": {
        "CANGJIE_PREBUILT_URL": "https://github.com/Zxilly/cangjie-mcp/releases/download/prebuilt-v1.0.7-zh/cangjie-index-v1.0.7-zh.tar.gz"
      }
    }
  }
}
```

</details>

<details>
<summary>Windsurf</summary>

在 Windsurf 配置目录创建或编辑 MCP 配置文件 `~/.codeium/windsurf/mcp_config.json`：

```json
{
  "mcpServers": {
    "cangjie": {
      "command": "uvx",
      "args": ["cangjie-mcp", "--rerank", "local"],
      "env": {
        "CANGJIE_PREBUILT_URL": "https://github.com/Zxilly/cangjie-mcp/releases/download/prebuilt-v1.0.7-zh/cangjie-index-v1.0.7-zh.tar.gz"
      }
    }
  }
}
```

</details>

<details>
<summary>VS Code (Copilot / Continue / Cline)</summary>

#### GitHub Copilot

在 VS Code 设置中添加（`settings.json`）：

```json
{
  "mcp": {
    "servers": {
      "cangjie": {
        "command": "uvx",
        "args": ["cangjie-mcp", "--rerank", "local"],
        "env": {
          "CANGJIE_PREBUILT_URL": "https://github.com/Zxilly/cangjie-mcp/releases/download/prebuilt-v1.0.7-zh/cangjie-index-v1.0.7-zh.tar.gz"
        }
      }
    }
  }
}
```

#### Continue

编辑 `~/.continue/config.yaml`：

```yaml
mcpServers:
  - name: cangjie
    command: uvx
    args:
      - cangjie-mcp
      - --rerank
      - local
    env:
      CANGJIE_PREBUILT_URL: https://github.com/Zxilly/cangjie-mcp/releases/download/prebuilt-v1.0.7-zh/cangjie-index-v1.0.7-zh.tar.gz
```

#### Cline

在 Cline 设置中添加 MCP 服务器，配置格式与 Cursor 相同。

</details>

<details>
<summary>Claude Desktop</summary>

编辑 Claude Desktop 配置文件：

- **macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`
- **Windows**: `%APPDATA%\Claude\claude_desktop_config.json`

```json
{
  "mcpServers": {
    "cangjie": {
      "command": "uvx",
      "args": ["cangjie-mcp", "--rerank", "local"],
      "env": {
        "CANGJIE_PREBUILT_URL": "https://github.com/Zxilly/cangjie-mcp/releases/download/prebuilt-v1.0.7-zh/cangjie-index-v1.0.7-zh.tar.gz"
      }
    }
  }
}
```

</details>

<details>
<summary>Zed</summary>

在 Zed 设置文件 `~/.config/zed/settings.json` 中添加：

```json
{
  "context_servers": {
    "cangjie": {
      "command": {
        "path": "uvx",
        "args": ["cangjie-mcp", "--rerank", "local"],
        "env": {
          "CANGJIE_PREBUILT_URL": "https://github.com/Zxilly/cangjie-mcp/releases/download/prebuilt-v1.0.7-zh/cangjie-index-v1.0.7-zh.tar.gz"
        }
      }
    }
  }
}
```

</details>

<details>
<summary>通用配置说明</summary>

所有配置中的关键参数：

| 参数 | 说明 |
|------|------|
| `command` | 运行命令，推荐使用 `uvx` 自动管理依赖 |
| `args` | 命令参数，`--rerank local` 启用本地重排序提升搜索质量 |
| `env.CANGJIE_PREBUILT_URL` | 预构建索引 URL，首次运行自动下载 |

如果已通过 `pip install cangjie-mcp` 安装，可将 `command` 改为 `cangjie-mcp`，`args` 中移除 `cangjie-mcp`：

```json
{
  "command": "cangjie-mcp",
  "args": ["--rerank", "local"],
  "env": { ... }
}
```

</details>

## 开发

### 环境要求

- Python 3.13+

### 安装开发依赖

```bash
uv sync --all-extras
```

### 运行测试

```bash
uv run pytest
uv run pytest tests/unit/
uv run pytest tests/integration/
```

### 代码检查

```bash
uv run ruff check src/ tests/
uv run ruff check src/ tests/ --fix
uv run mypy src/
uv run pyright src/
```

### 项目结构

```
src/cangjie_mcp/
├── __init__.py
├── cli.py              # CLI 入口 (typer)
├── config.py           # 配置管理 (pydantic-settings)
├── utils.py            # 通用工具函数
├── indexer/            # 文档索引模块
│   ├── chunker.py      # 文档分块
│   ├── embeddings.py   # 嵌入模型抽象层
│   ├── loader.py       # 文档加载器
│   ├── multi_store.py  # 多索引管理
│   ├── reranker.py     # 重排序抽象层
│   └── store.py        # 向量存储 (ChromaDB)
├── prebuilt/           # 预构建索引模块
│   └── manager.py      # 索引构建/下载/安装
├── repo/               # 文档仓库管理
│   └── git_manager.py  # Git 操作
└── server/             # MCP 服务器
    ├── app.py          # MCP 应用 (FastMCP)
    ├── http.py         # HTTP 服务器 (多索引)
    └── tools.py        # MCP 工具实现
```

## 许可证

MIT License
