# Cangjie MCP Server

仓颉编程语言的 MCP (Model Context Protocol) 服务器，提供文档搜索和代码智能功能。

## 功能

- **文档搜索**: 基于向量检索的仓颉语言文档搜索
- **代码智能**: 基于 LSP 的代码补全、跳转定义、查找引用等功能
- **客户端-服务器架构**: 支持通过 HTTP 服务器分离索引和查询

## 安装

```bash
pip install cangjie-mcp
```

或使用 uvx 直接运行（推荐）：

```bash
uvx cangjie-mcp  # 启动 MCP 服务器（包含文档搜索 + 代码智能）
```

## 架构

cangjie-mcp 支持两种运行模式：

### 本地模式（默认）

MCP 服务器在本地加载嵌入模型和 ChromaDB 索引，直接处理查询。

```bash
cangjie-mcp
```

### 客户端-服务器模式

将索引和嵌入模型放在独立的 HTTP 服务器上，MCP 客户端通过 `--server-url` 连接，无需本地加载模型。

```bash
# 终端 1：启动 HTTP 查询服务器
cangjie-mcp server --embedding local --port 8765

# 终端 2：启动 MCP 服务器，连接远程索引
cangjie-mcp --server-url http://localhost:8765
```

## 快速配置

公共文档服务器已部署在 `https://cj-mcp.learningman.top`，连接后无需本地加载嵌入模型，开箱即用。

> **注意**：LSP 功能需要已安装仓颉 SDK，请将 `/path/to/cangjie-sdk` 替换为实际路径，或设置 `CANGJIE_HOME` 环境变量。

<details>
<summary>Claude Code（推荐）</summary>

```bash
claude mcp add \
  -e CANGJIE_SERVER_URL=https://cj-mcp.learningman.top \
  -e CANGJIE_HOME=/path/to/cangjie-sdk \
  cangjie -- uvx cangjie-mcp
```

</details>

<details>
<summary>Cursor / Windsurf / Claude Desktop</summary>

配置文件路径：
- **Cursor**: `~/.cursor/mcp.json`
- **Windsurf**: `~/.codeium/windsurf/mcp_config.json`
- **Claude Desktop (macOS)**: `~/Library/Application Support/Claude/claude_desktop_config.json`
- **Claude Desktop (Windows)**: `%APPDATA%\Claude\claude_desktop_config.json`

```json
{
  "mcpServers": {
    "cangjie": {
      "command": "uvx",
      "args": ["cangjie-mcp"],
      "env": {
        "CANGJIE_SERVER_URL": "https://cj-mcp.learningman.top",
        "CANGJIE_HOME": "/path/to/cangjie-sdk"
      }
    }
  }
}
```

</details>

<details>
<summary>VS Code (GitHub Copilot)</summary>

`settings.json`:

```json
{
  "mcp": {
    "servers": {
      "cangjie": {
        "command": "uvx",
        "args": ["cangjie-mcp"],
        "env": {
          "CANGJIE_SERVER_URL": "https://cj-mcp.learningman.top",
          "CANGJIE_HOME": "/path/to/cangjie-sdk"
        }
      }
    }
  }
}
```

</details>

<details>
<summary>Zed</summary>

`~/.config/zed/settings.json`:

```json
{
  "context_servers": {
    "cangjie": {
      "command": {
        "path": "uvx",
        "args": ["cangjie-mcp"],
        "env": {
          "CANGJIE_SERVER_URL": "https://cj-mcp.learningman.top",
          "CANGJIE_HOME": "/path/to/cangjie-sdk"
        }
      }
    }
  }
}
```

</details>

<details>
<summary>本地模式（不使用远程服务器）</summary>

如需完全离线使用或自建索引，可不设置 `CANGJIE_SERVER_URL`，MCP 会在本地加载嵌入模型和索引：

```bash
claude mcp add \
  -e CANGJIE_RERANK_TYPE=local \
  -e CANGJIE_HOME=/path/to/cangjie-sdk \
  cangjie -- uvx cangjie-mcp
```

</details>

## 可用工具

### 文档搜索

| 工具名称 | 功能 |
|---------|------|
| `cangjie_search_docs` | 语义搜索仓颉文档 |
| `cangjie_get_topic` | 获取指定主题的完整内容 |
| `cangjie_list_topics` | 列出所有可用主题 |

### 代码智能

> 需要设置 `CANGJIE_HOME` 环境变量指向仓颉 SDK 路径，LSP 工具才会注册。

| 工具名称 | 功能 |
|---------|------|
| `cangjie_lsp_definition` | 跳转到符号定义 |
| `cangjie_lsp_references` | 查找符号的所有引用 |
| `cangjie_lsp_hover` | 获取符号的类型信息和文档 |
| `cangjie_lsp_symbols` | 列出文档中的所有符号 |
| `cangjie_lsp_diagnostics` | 获取文件的错误和警告 |
| `cangjie_lsp_completion` | 获取代码补全建议 |

## 命令行参考

### cangjie-mcp

启动 MCP 服务器，同时提供文档搜索和 LSP 代码智能功能。LSP 功能在设置了 `CANGJIE_HOME` 环境变量时自动启用。

```bash
cangjie-mcp [OPTIONS]
```

### 选项

| CLI 参数 | 环境变量 | 默认值 | 说明 |
|---------|---------|-------|------|
| `-v, --version` | - | - | 显示版本并退出 |
| `--log-file PATH` | `CANGJIE_LOG_FILE` | - | 日志文件路径 |
| `--debug / --no-debug` | `CANGJIE_DEBUG` | `--no-debug` | 启用调试模式，将 stdio 流量写入日志文件 |
| `-V, --docs-version TEXT` | `CANGJIE_DOCS_VERSION` | `latest` | 文档版本 (git tag) |
| `-l, --lang TEXT` | `CANGJIE_DOCS_LANG` | `zh` | 文档语言 (`zh` / `en`) |
| `-e, --embedding TEXT` | `CANGJIE_EMBEDDING_TYPE` | `local` | 向量化类型 (`local` / `openai`) |
| `--local-model TEXT` | `CANGJIE_LOCAL_MODEL` | `paraphrase-multilingual-MiniLM-L12-v2` | 本地 HuggingFace 向量化模型 |
| `--openai-api-key TEXT` | `OPENAI_API_KEY` | - | OpenAI API 密钥 |
| `--openai-base-url TEXT` | `OPENAI_BASE_URL` | `https://api.openai.com/v1` | OpenAI API 基础 URL |
| `--openai-model TEXT` | `OPENAI_EMBEDDING_MODEL` | `text-embedding-3-small` | OpenAI 向量化模型 |
| `-r, --rerank TEXT` | `CANGJIE_RERANK_TYPE` | `none` | 重排序类型 (`none` / `local` / `openai`) |
| `--rerank-model TEXT` | `CANGJIE_RERANK_MODEL` | `BAAI/bge-reranker-v2-m3` | 重排序模型 |
| `--rerank-top-k INT` | `CANGJIE_RERANK_TOP_K` | `5` | 重排序后返回结果数 |
| `--rerank-initial-k INT` | `CANGJIE_RERANK_INITIAL_K` | `20` | 重排序前候选数 |
| `--chunk-size INT` | `CANGJIE_CHUNK_MAX_SIZE` | `6000` | 最大分块大小（字符数） |
| `-d, --data-dir PATH` | `CANGJIE_DATA_DIR` | `~/.cangjie-mcp` | 数据目录路径 |
| `--server-url TEXT` | `CANGJIE_SERVER_URL` | - | 远程查询服务器 URL |

LSP 功能通过以下环境变量控制：

| 环境变量 | 说明 |
|---------|------|
| `CANGJIE_HOME` | 仓颉 SDK 路径，设置后自动启用 LSP 工具 |

### cangjie-mcp server

启动 HTTP 查询服务器，加载嵌入模型和 ChromaDB 索引，通过 HTTP 提供查询服务。

```bash
cangjie-mcp server [OPTIONS]
```

支持所有与 `cangjie-mcp` 相同的索引选项，以及：

| CLI 参数 | 环境变量 | 默认值 | 说明 |
|---------|---------|-------|------|
| `--host TEXT` | `CANGJIE_SERVER_HOST` | `127.0.0.1` | HTTP 服务器监听地址 |
| `-p, --port INT` | `CANGJIE_SERVER_PORT` | `8765` | HTTP 服务器监听端口 |

#### HTTP API

| 方法 | 路径 | 说明 |
|------|------|------|
| `GET` | `/health` | 健康检查 |
| `GET` | `/info` | 索引元数据 |
| `POST` | `/search` | 向量搜索 |
| `GET` | `/topics` | 列出所有分类和主题 |
| `GET` | `/topics/{category}/{topic}` | 获取文档内容 |

### 调试与日志

`--log-file` 和 `--debug` 配合使用，可以帮助排查 MCP 通信问题：

```bash
# 记录应用日志到文件
cangjie-mcp --log-file /tmp/cangjie.log

# 调试模式：额外记录 MCP stdio 协议流量
cangjie-mcp --log-file /tmp/cangjie.log --debug

# 通过环境变量配置
CANGJIE_LOG_FILE=/tmp/cangjie.log CANGJIE_DEBUG=1 cangjie-mcp
```

## 许可证

MIT License
