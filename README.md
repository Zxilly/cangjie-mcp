# Cangjie MCP Server

[![CI](https://github.com/Zxilly/cangjie-mcp/actions/workflows/ci.yml/badge.svg)](https://github.com/Zxilly/cangjie-mcp/actions/workflows/ci.yml)
[![PyPI](https://img.shields.io/pypi/v/cangjie-mcp)](https://pypi.org/project/cangjie-mcp/)
[![Python Versions](https://img.shields.io/pypi/pyversions/cangjie-mcp)](https://pypi.org/project/cangjie-mcp/)
[![GitHub Release](https://img.shields.io/github/v/release/Zxilly/cangjie-mcp)](https://github.com/Zxilly/cangjie-mcp/releases)
[![License](https://img.shields.io/github/license/Zxilly/cangjie-mcp)](LICENSE)

仓颉编程语言的 MCP (Model Context Protocol) 服务器，提供文档搜索和代码智能功能。

## 功能

- **文档搜索**: 基于向量检索的仓颉语言文档搜索
- **代码智能**: 基于 LSP 的代码补全、跳转定义、查找引用等功能
- **可选远程文档服务**: 支持连接远程文档/索引服务，减少本地资源占用，适合开箱即用或团队共享

## 安装

```bash
pip install cangjie-mcp
```

或使用 uvx 直接运行（推荐）：

```bash
uvx cangjie-mcp  # 启动 MCP 服务器
```

也可以通过 npm 安装 CLI：

```bash
npm install -g cangjie-mcp
```

安装后会提供两个入口：

- `cangjie`
- `cangjie-mcp`，等价于 `cangjie mcp`

npm 默认优先使用预编译二进制；当当前平台没有匹配包，或 Linux 的 glibc 低于 `2.28` 时，会自动回退到本地源码构建。你也可以显式强制源码构建：

```bash
CANGJIE_MCP_FORCE_BUILD=1 npm install -g cangjie-mcp
```

### 安装并指定 Rust feature

当平台没有预编译 wheel，或你希望按需启用 GPU/NPU 等加速后端时，可强制从 sdist 构建并传入 maturin 构建参数：

```bash
pip install --no-binary cangjie-mcp cangjie-mcp \
  --config-settings=build-args="--features local"
```


可用 feature（传给 `cangjie-cli`）：

- `local`：本地向量化（CPU；默认构建的二进制已启用）
- `legacy`：本地向量化改用 `ort-tract` 后端（适配旧 glibc 构建）
- `local-cuda` / `local-cudnn`：启用 CUDA/CUDNN 后端
- `local-metal`：启用 Apple Metal 后端
- `local-mkl` / `local-accelerate`：启用 MKL / Accelerate 后端


## 架构

cangjie-mcp 支持两种运行模式：

### 本地模式（默认）

MCP 服务器在本地加载检索索引（BM25 + 向量索引），直接处理查询。

```bash
cangjie mcp           # 或 uvx cangjie-mcp
```

### 远程文档服务模式（可选）

如果你不想在本机下载/加载向量模型与索引（或希望多人/多台机器共享同一套索引），可以把检索能力以 HTTP 的方式独立部署。之后 MCP 只需要通过 `--server-url` 连接，就能直接使用文档检索能力。

```bash
# 终端 1：启动 HTTP 查询服务器
cangjie-mcp-server --embedding local --port 8765

# 终端 2：启动 MCP 服务器，连接远程索引
cangjie mcp --server-url http://localhost:8765
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

## AI 编程助手 Skill

项目根目录包含 [`SKILL.md`](SKILL.md)，遵循 [vercel-labs/skills](https://github.com/vercel-labs/ai-sdk-preview-tool-call) 规范。支持该规范的 AI 编程助手（如 Claude Code、Cursor 等）会自动识别并加载该文件，获取仓颉语言的语法速查、关键差异和常见陷阱等知识，无需额外配置。

如果你的 AI 助手支持 skill 规范，只需将本项目作为 MCP 服务器接入，助手即可同时获得：
- **SKILL.md** 中的仓颉语言知识（语法、类型系统、并发模型等）
- **MCP 工具** 提供的文档搜索和代码智能能力

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
| `cangjie_lsp` | 统一 LSP 入口，通过 `operation` 执行 definition、references、hover、document_symbol、diagnostics、workspace_symbol、incoming/outgoing calls、type hierarchy、rename 和 completion |

## 命令行参考

### cangjie

安装 Python 包后，`cangjie` 命令可直接使用。

```bash
cangjie mcp                    # 启动 MCP stdio 服务器
cangjie query "泛型"           # CLI 搜索（自动启动后台 daemon）
cangjie topic functions        # 获取完整文档
cangjie topics -c stdlib       # 列出 stdlib 分类下的主题
cangjie lsp hover main.cj --symbol main  # LSP 操作
cangjie index                  # 构建搜索索引
cangjie config init            # 生成默认配置文件
```

`cangjie mcp` 和 `cangjie index` 接受完整的索引/嵌入/网络选项（通过 `cangjie mcp --help` 查看）。其他子命令的设置统一从配置文件加载，运行 `cangjie config path` 查看路径。

### 全局选项

| CLI 参数 | 环境变量 | 说明 |
|---------|---------|------|
| `--log-file PATH` | `CANGJIE_LOG_FILE` | 日志文件路径 |
| `--debug` | `CANGJIE_DEBUG` | 启用调试模式 |
| `-h, --help` | - | 显示帮助 |
| `-V, --version` | - | 显示版本 |

### 环境变量

| 环境变量 | 说明 |
|---------|------|
| `CANGJIE_HOME` | 仓颉 SDK 路径，设置后自动启用 LSP 工具 |

### cangjie-mcp-server

启动 HTTP 查询服务器，加载本地检索索引（BM25 + 向量索引），通过 HTTP 提供查询服务。该服务器为独立二进制，需单独构建。

```bash
cangjie-mcp-server [OPTIONS]
```

支持所有与 `cangjie` 相同的索引选项，以及：

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

### Daemon 管理

CLI 工具命令（`query`、`topic`、`lsp` 等）会自动在后台启动 daemon 进程，复用已初始化的服务实例以加速响应。daemon 空闲超时后自动退出。

```bash
cangjie daemon status           # 查看 daemon 状态
cangjie daemon stop             # 停止 daemon
cangjie daemon logs --tail 50   # 查看日志
cangjie daemon logs --follow    # 实时跟踪日志
```

### 调试与日志

`--log-file` 和 `--debug` 配合使用，可以帮助排查 MCP 通信问题：

```bash
# 记录应用日志到文件
cangjie mcp --log-file /tmp/cangjie.log

# 调试模式：额外记录 MCP stdio 协议流量
cangjie mcp --log-file /tmp/cangjie.log --debug

# 通过环境变量配置
CANGJIE_LOG_FILE=/tmp/cangjie.log CANGJIE_DEBUG=1 cangjie mcp
```

## 许可证

MIT License
