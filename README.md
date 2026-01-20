# Cangjie MCP Server

仓颉编程语言文档的 MCP (Model Context Protocol) 服务器。

## 安装

```bash
pip install cangjie-mcp
```

## 命令行帮助

```
Usage: cangjie-mcp [OPTIONS] COMMAND [ARGS]...

 MCP server for Cangjie programming language documentation

Options:
  -v, --version               Show version and exit
  -V, --docs-version TEXT     Documentation version (git tag) [env: CANGJIE_DOCS_VERSION]
  -l, --lang TEXT             Documentation language (zh/en) [env: CANGJIE_DOCS_LANG]
  -e, --embedding TEXT        Embedding type (local/openai) [env: CANGJIE_EMBEDDING_TYPE]
  --local-model TEXT          Local HuggingFace embedding model name [env: CANGJIE_LOCAL_MODEL]
  --openai-api-key TEXT       OpenAI API key [env: OPENAI_API_KEY]
  --openai-base-url TEXT      OpenAI API base URL [env: OPENAI_BASE_URL]
  --openai-model TEXT         OpenAI embedding model [env: OPENAI_EMBEDDING_MODEL]
  -r, --rerank TEXT           Rerank type (none/local/openai) [env: CANGJIE_RERANK_TYPE]
  --rerank-model TEXT         Rerank model name [env: CANGJIE_RERANK_MODEL]
  --rerank-top-k INTEGER      Number of results after reranking [env: CANGJIE_RERANK_TOP_K]
  --rerank-initial-k INTEGER  Number of candidates before reranking [env: CANGJIE_RERANK_INITIAL_K]
  --chunk-size INTEGER        Max chunk size in characters [env: CANGJIE_CHUNK_MAX_SIZE]
  -d, --data-dir PATH         Data directory path [env: CANGJIE_DATA_DIR]
  --help                      Show this message and exit.

Commands:
  serve      Start the HTTP server.
  prebuilt   Prebuilt index management commands
```

### serve

```
Usage: cangjie-mcp serve [OPTIONS]

 Start the HTTP server.

Options:
  -i, --indexes TEXT        Comma-separated list of URLs to prebuilt index archives [env: CANGJIE_INDEXES]
  -H, --host TEXT           HTTP server host address [env: CANGJIE_HTTP_HOST]
  -p, --port INTEGER        HTTP server port [env: CANGJIE_HTTP_PORT]
  -e, --embedding TEXT      Embedding type (local/openai) [env: CANGJIE_EMBEDDING_TYPE]
  --local-model TEXT        Local HuggingFace embedding model name [env: CANGJIE_LOCAL_MODEL]
  --openai-api-key TEXT     OpenAI API key [env: OPENAI_API_KEY]
  --openai-base-url TEXT    OpenAI API base URL [env: OPENAI_BASE_URL]
  --openai-model TEXT       OpenAI embedding model [env: OPENAI_EMBEDDING_MODEL]
  -r, --rerank TEXT         Rerank type (none/local/openai) [env: CANGJIE_RERANK_TYPE]
  --rerank-model TEXT       Rerank model name [env: CANGJIE_RERANK_MODEL]
  -d, --data-dir PATH       Data directory path [env: CANGJIE_DATA_DIR]
  --help                    Show this message and exit.
```

### prebuilt

```
Usage: cangjie-mcp prebuilt [OPTIONS] COMMAND [ARGS]...

 Prebuilt index management commands

Commands:
  download   Download a prebuilt index.
  build      Build a prebuilt index archive.
  list       List available prebuilt indexes.
```

## 一键配置

### Claude Code

```bash
claude mcp add \
  -e CANGJIE_PREBUILT_URL=https://github.com/Zxilly/cangjie-mcp/releases/download/prebuilt-v1.0.7-zh/cangjie-index-v1.0.7-zh.tar.gz \
  -e CANGJIE_RERANK_TYPE=local \
  cangjie -- uvx cangjie-mcp
```

### Cursor

`~/.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "cangjie": {
      "command": "uvx",
      "args": ["cangjie-mcp"],
      "env": {
        "CANGJIE_PREBUILT_URL": "https://github.com/Zxilly/cangjie-mcp/releases/download/prebuilt-v1.0.7-zh/cangjie-index-v1.0.7-zh.tar.gz",
        "CANGJIE_RERANK_TYPE": "local"
      }
    }
  }
}
```

### Windsurf

`~/.codeium/windsurf/mcp_config.json`:

```json
{
  "mcpServers": {
    "cangjie": {
      "command": "uvx",
      "args": ["cangjie-mcp"],
      "env": {
        "CANGJIE_PREBUILT_URL": "https://github.com/Zxilly/cangjie-mcp/releases/download/prebuilt-v1.0.7-zh/cangjie-index-v1.0.7-zh.tar.gz",
        "CANGJIE_RERANK_TYPE": "local"
      }
    }
  }
}
```

### VS Code (GitHub Copilot)

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
          "CANGJIE_RERANK_TYPE": "local"
        }
      }
    }
  }
}
```

### Claude Desktop

- **macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`
- **Windows**: `%APPDATA%\Claude\claude_desktop_config.json`

```json
{
  "mcpServers": {
    "cangjie": {
      "command": "uvx",
      "args": ["cangjie-mcp"],
      "env": {
        "CANGJIE_PREBUILT_URL": "https://github.com/Zxilly/cangjie-mcp/releases/download/prebuilt-v1.0.7-zh/cangjie-index-v1.0.7-zh.tar.gz",
        "CANGJIE_RERANK_TYPE": "local"
      }
    }
  }
}
```

### Zed

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
          "CANGJIE_RERANK_TYPE": "local"
        }
      }
    }
  }
}
```

## 许可证

MIT License
