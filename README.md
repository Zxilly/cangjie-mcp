# Cangjie MCP Server

[![CI](https://github.com/Zxilly/cangjie-mcp/actions/workflows/ci.yml/badge.svg)](https://github.com/Zxilly/cangjie-mcp/actions/workflows/ci.yml)
[![PyPI](https://img.shields.io/pypi/v/cangjie-mcp)](https://pypi.org/project/cangjie-mcp/)
[![Python Versions](https://img.shields.io/pypi/pyversions/cangjie-mcp)](https://pypi.org/project/cangjie-mcp/)
[![GitHub Release](https://img.shields.io/github/v/release/Zxilly/cangjie-mcp)](https://github.com/Zxilly/cangjie-mcp/releases)
[![License](https://img.shields.io/github/license/Zxilly/cangjie-mcp)](LICENSE)

仓颉编程语言的 MCP (Model Context Protocol) 服务器，提供文档搜索和代码智能功能。

## 功能

- **文档搜索**: 基于向量检索的仓颉语言文档搜索
- **代码智能**: 基于 LSP 的跳转定义、查找引用、悬停信息等功能
- **可选远程文档服务**: 支持连接远程文档/索引服务，减少本地资源占用，适合开箱即用或团队共享

## 安装

你可以根据偏好的包管理器，选择通过 PyPI (Python) 或 npm (Node.js) 进行安装。

### 通过 PyPI 安装 (Python)

```bash
pip install cangjie-mcp
```

#### 安装并指定 Rust feature

当平台没有预编译 wheel，或你希望按需启用 GPU/NPU 等加速后端时，可强制从 sdist 构建并传入 maturin 构建参数：

```bash
pip install --no-binary cangjie-mcp cangjie-mcp \
  --config-settings=build-args="--features local"
```


可用 feature（传给 `cangjie-mcp-cli`）：

- `local`：本地向量化（CPU；默认构建的二进制已启用）
- `legacy`：本地向量化改用 `ort-tract` 后端（适配旧 glibc 构建）
- `local-cuda` / `local-cudnn`：启用 CUDA/CUDNN 后端
- `local-metal`：启用 Apple Metal 后端
- `local-mkl` / `local-accelerate`：启用 MKL / Accelerate 后端

### 通过 npm 安装 (Node.js)

```bash
npm install -g cangjie-mcp
```

npm 默认优先使用预编译二进制；当当前平台没有匹配包，或 Linux 的 glibc 低于 `2.28` 时，会自动回退到本地源码构建。你也可以显式强制源码构建：

```bash
CANGJIE_MCP_FORCE_BUILD=1 npm install -g cangjie-mcp
```


## 架构

cangjie-mcp 支持两种运行模式：

### 本地模式（默认）

MCP 服务器在本地加载检索索引（BM25 + 向量索引），直接处理查询。

```bash
cangjie-mcp               # 或 uvx cangjie-mcp
```

### 远程服务模式（可选）

如果你不想在本机下载/加载向量模型与索引（或希望多人/多台机器共享同一套索引），可以把检索能力独立部署为 `cangjie-mcp-server`，同时对外提供：

- **HTTP 查询 API**，供本地 `cangjie-mcp --server-url` 连接（LSP 工具仍在本地可用）
- **Remote MCP 端点**，MCP 客户端无需本地运行 `cangjie-mcp` 即可直接连接
  - Streamable HTTP（默认 `/mcp`）— [MCP 规范推荐的现代传输方式](https://modelcontextprotocol.io/specification/2025-03-26/basic/transports#streamable-http)
  - SSE（默认 `/sse`）— 旧版兼容传输

> **注意**：`cangjie-mcp-server` 为独立二进制，不随 PyPI / npm 发行，需单独从源码构建。Remote MCP 模式下 LSP 工具（`cangjie_lsp`）不可用 —— 它依赖本地文件系统和仓颉 SDK，仅在 stdio 模式（`cangjie-mcp`）中提供。

```bash
# 启动服务器（同时暴露 HTTP API、Streamable HTTP MCP、SSE MCP）
cangjie-mcp-server --embedding local --port 8765

# 方式 A：本地 stdio MCP 连接远程索引（保留 LSP）
cangjie-mcp --server-url http://localhost:8765

# 方式 B：客户端直接连接 Remote MCP（无需本地进程）
#   Streamable HTTP: http://localhost:8765/mcp
#   SSE:             http://localhost:8765/sse
```

## 快速配置

项目提供两部分能力，可独立安装：**MCP 服务器**（文档检索 + LSP 工具）和 **Skill**（仓颉语言速查知识）。推荐搭配使用。

### MCP 服务器

公共文档服务器已部署在 `https://cj-mcp.learningman.top`，直接使用 Streamable HTTP 远程 MCP，无需在本地安装任何包，也无需加载嵌入模型，开箱即用：

```bash
npx add-mcp https://cj-mcp.learningman.top/mcp
```

如需 LSP 代码智能工具（需要本地仓颉 SDK），改用 stdio 模式，通过 `uvx` 拉起本地进程并连接远程检索服务：

```bash
npx add-mcp "uvx cangjie-mcp" \
  --env "CANGJIE_SERVER_URL=https://cj-mcp.learningman.top" \
  --env "CANGJIE_HOME=/path/to/cangjie-sdk"
```

完全离线 / 自建索引（不连接远程服务器，本地加载嵌入模型）：

```bash
npx add-mcp "uvx cangjie-mcp" \
  --env "CANGJIE_RERANK_TYPE=local" \
  --env "CANGJIE_HOME=/path/to/cangjie-sdk"
```

> `add-mcp` 会自动识别当前环境中的 AI 编程助手（Claude Code、Cursor、Windsurf、VS Code、Zed、Claude Desktop 等）并写入对应配置，详见 [neondatabase/add-mcp](https://github.com/neondatabase/add-mcp)。使用 `-g` 全局安装，`-a <agent>` 指定目标，`--all` 写入所有受支持的客户端。

### Skill

本项目提供仓颉语言的 AI 编程助手技能，包含语法速查、关键差异和常见陷阱等知识。项目根目录的 [`SKILL.md`](SKILL.md) 遵循 skills 规范，可直接通过以下命令安装到你的项目：

```bash
npx skills add Zxilly/cangjie-mcp
```

## 可用工具

### 文档搜索

| 工具名称 | 功能 |
|---------|------|
| `cangjie_search_docs` | 语义搜索仓颉文档 |

### 代码智能

> 需要设置 `CANGJIE_HOME` 环境变量指向仓颉 SDK 路径，LSP 工具才会注册。

| 工具名称 | 功能 |
|---------|------|
| `cangjie_lsp` | 统一 LSP 入口，通过 `operation` 执行 definition、references、hover、document_symbol、diagnostics、workspace_symbol、incoming/outgoing calls 和 type hierarchy |

## 命令行参考

### cangjie-mcp

安装 Python 包后，`cangjie-mcp` 命令可直接使用。

```bash
cangjie-mcp                        # 启动 MCP stdio 服务器（无参数时默认行为）
cangjie-mcp query "泛型"           # CLI 搜索（自动启动后台 daemon）
cangjie-mcp lsp hover main.cj --symbol main  # LSP 操作
cangjie-mcp index                  # 构建搜索索引
cangjie-mcp config init            # 生成默认配置文件
```

`cangjie-mcp` 和 `cangjie-mcp index` 接受完整的索引/嵌入/网络选项（通过 `cangjie-mcp --help` 查看）。其他子命令的设置统一从配置文件加载，运行 `cangjie-mcp config path` 查看路径。

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

启动 HTTP 查询服务器和 Remote MCP 服务器，加载本地检索索引（BM25 + 向量索引），通过 HTTP 提供查询服务，同时提供 Streamable HTTP（`/mcp`）和旧版 SSE（`/sse`）两种 MCP 传输方式。该服务器为独立二进制，需单独构建。

```bash
cangjie-mcp-server [OPTIONS]
```

支持所有与 `cangjie-mcp` 相同的索引选项，以及：

| CLI 参数 | 环境变量 | 默认值 | 说明 |
|---------|---------|-------|------|
| `--host TEXT` | `CANGJIE_SERVER_HOST` | `127.0.0.1` | HTTP 服务器监听地址 |
| `-p, --port INT` | `CANGJIE_SERVER_PORT` | `8765` | HTTP 服务器监听端口 |
| `--mcp-path TEXT` | `CANGJIE_MCP_PATH` | `/mcp` | Streamable HTTP MCP 端点挂载路径 |
| `--no-mcp` | `CANGJIE_NO_MCP` | - | 禁用 Streamable HTTP MCP 端点 |
| `--no-sse` | `CANGJIE_NO_SSE` | - | 禁用旧版 SSE 传输端点 |

#### HTTP API

| 方法 | 路径 | 说明 |
|------|------|------|
| `GET` | `/health` | 健康检查 |
| `GET` | `/info` | 索引元数据 |
| `POST` | `/search` | 向量搜索 |

#### MCP 端点

默认同时提供两种 MCP 传输方式，暴露 `cangjie_search_docs` 工具：

| 传输方式 | 端点 | 说明 |
|---------|------|------|
| Streamable HTTP | `/mcp`（可通过 `--mcp-path` 配置） | 现代 MCP 传输，使用 `--no-mcp` 禁用 |
| SSE（旧版） | `GET /sse` + `POST /sse` | 兼容旧客户端，使用 `--no-sse` 禁用 |

> **注意**：Remote MCP 模式下 LSP 工具不可用，仅提供文档搜索相关工具。

### Daemon 管理

CLI 工具命令（`query`、`lsp` 等）会自动在后台启动 daemon 进程，复用已初始化的服务实例以加速响应。daemon 空闲超时后自动退出。

```bash
cangjie-mcp daemon status           # 查看 daemon 状态
cangjie-mcp daemon stop             # 停止 daemon
cangjie-mcp daemon logs --tail 50   # 查看日志
cangjie-mcp daemon logs --follow    # 实时跟踪日志
```

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
