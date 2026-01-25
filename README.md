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

### cangjie-mcp

默认启动聚合服务器，同时提供文档搜索和 LSP 代码智能功能。

```
Usage: cangjie-mcp [OPTIONS] COMMAND [ARGS]...

Options:
  -v, --version               Show version and exit
  -V, --docs-version TEXT     Documentation version [env: CANGJIE_DOCS_VERSION]
  -l, --lang TEXT             Documentation language (zh/en) [env: CANGJIE_DOCS_LANG]
  -e, --embedding TEXT        Embedding type (local/openai) [env: CANGJIE_EMBEDDING_TYPE]
  -r, --rerank TEXT           Rerank type (none/local/openai) [env: CANGJIE_RERANK_TYPE]
  -d, --data-dir PATH         Data directory path [env: CANGJIE_DATA_DIR]
  --help                      Show this message and exit.

Commands:
  docs   Documentation search MCP server (standalone)
  lsp    LSP code intelligence MCP server (standalone)
```

### 子命令

如果只需要单独功能，可以使用子命令：

```bash
cangjie-mcp docs  # 仅文档搜索
cangjie-mcp lsp   # 仅代码智能
```

## 许可证

MIT License
