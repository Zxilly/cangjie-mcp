# Cangjie MCP Server

仓颉编程语言的 MCP (Model Context Protocol) 服务器，提供文档搜索和代码智能功能。

## 功能

- **文档搜索**: 基于向量检索的仓颉语言文档搜索
- **代码智能**: 基于 LSP 的代码补全、跳转定义、查找引用等功能

## 安装

```bash
pip install cangjie-mcp
```

或使用 uvx 直接运行（推荐）：

```bash
uvx cangjie-mcp  # 启动聚合服务器（包含文档搜索 + 代码智能）
```

## 快速配置

> **注意**：LSP 功能需要已安装仓颉 SDK，请将 `/path/to/cangjie-sdk` 替换为实际路径，或设置 `CANGJIE_HOME` 环境变量。

<details>
<summary>Claude Code</summary>

```bash
claude mcp add \
  -e CANGJIE_PREBUILT_URL=https://github.com/Zxilly/cangjie-mcp/releases/download/prebuilt-v1.0.7-zh/cangjie-index-v1.0.7-zh.tar.gz \
  -e CANGJIE_RERANK_TYPE=local \
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
        "CANGJIE_PREBUILT_URL": "https://github.com/Zxilly/cangjie-mcp/releases/download/prebuilt-v1.0.7-zh/cangjie-index-v1.0.7-zh.tar.gz",
        "CANGJIE_RERANK_TYPE": "local",
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
          "CANGJIE_PREBUILT_URL": "https://github.com/Zxilly/cangjie-mcp/releases/download/prebuilt-v1.0.7-zh/cangjie-index-v1.0.7-zh.tar.gz",
          "CANGJIE_RERANK_TYPE": "local",
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
          "CANGJIE_PREBUILT_URL": "https://github.com/Zxilly/cangjie-mcp/releases/download/prebuilt-v1.0.7-zh/cangjie-index-v1.0.7-zh.tar.gz",
          "CANGJIE_RERANK_TYPE": "local",
          "CANGJIE_HOME": "/path/to/cangjie-sdk"
        }
      }
    }
  }
}
```

</details>

## 可用工具

### 文档搜索

| 工具名称 | 功能 |
|---------|------|
| `cangjie_search_docs` | 语义搜索仓颉文档 |
| `cangjie_get_topic` | 获取指定主题的完整内容 |
| `cangjie_list_topics` | 列出所有可用主题 |
| `cangjie_get_code_examples` | 获取代码示例 |
| `cangjie_get_tool_usage` | 获取工具使用说明 |

### 代码智能

| 工具名称 | 功能 |
|---------|------|
| `cangjie_lsp_definition` | 跳转到符号定义 |
| `cangjie_lsp_references` | 查找符号的所有引用 |
| `cangjie_lsp_hover` | 获取符号的类型信息和文档 |
| `cangjie_lsp_symbols` | 列出文档中的所有符号 |
| `cangjie_lsp_diagnostics` | 获取文件的错误和警告 |
| `cangjie_lsp_completion` | 获取代码补全建议 |

## 命令行参考

### cangjie-mcp（默认命令）

启动聚合服务器，同时提供文档搜索和 LSP 代码智能功能。

```bash
cangjie-mcp [OPTIONS]
cangjie-mcp docs [OPTIONS]   # 仅文档搜索
cangjie-mcp lsp [OPTIONS]    # 仅代码智能
```

### 通用选项

以下选项适用于 `cangjie-mcp`、`cangjie-mcp docs` 和 `cangjie-mcp lsp`：

| CLI 参数 | 环境变量 | 默认值 | 说明 |
|---------|---------|-------|------|
| `--log-file PATH` | `CANGJIE_LOG_FILE` | - | 日志文件路径 |
| `--debug / --no-debug` | `CANGJIE_DEBUG` | `--no-debug` | 启用调试模式，将 stdio 流量写入日志文件 |

### 文档选项

以下选项适用于 `cangjie-mcp` 和 `cangjie-mcp docs`：

| CLI 参数 | 环境变量 | 默认值 | 说明 |
|---------|---------|-------|------|
| `-v, --version` | - | - | 显示版本并退出 |
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

### LSP 选项

以下选项仅适用于 `cangjie-mcp lsp`：

| CLI 参数 | 环境变量 | 默认值 | 说明 |
|---------|---------|-------|------|
| `-v, --version` | - | - | 显示版本并退出 |
| `-w, --workspace PATH` | `CANGJIE_WORKSPACE` | 当前目录 | 工作区根路径 |
| `-s, --sdk PATH` | `CANGJIE_HOME` | - | 仓颉 SDK 路径 |
| `--log / --no-log` | `CANGJIE_LSP_LOG` | `--no-log` | 启用 LSP 服务器日志 |
| `--log-path PATH` | `CANGJIE_LSP_LOG_PATH` | - | LSP 日志文件目录 |
| `-t, --timeout INT` | `CANGJIE_LSP_TIMEOUT` | `45000` | LSP 初始化超时（毫秒） |

### 预构建索引管理

```bash
cangjie-mcp docs prebuilt download [OPTIONS]  # 下载预构建索引
cangjie-mcp docs prebuilt build [OPTIONS]     # 构建预构建索引
cangjie-mcp docs prebuilt list [OPTIONS]      # 列出可用索引
```

| CLI 参数 | 环境变量 | 说明 |
|---------|---------|------|
| `-u, --url TEXT` | `CANGJIE_PREBUILT_URL` | 预构建索引下载 URL |

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
