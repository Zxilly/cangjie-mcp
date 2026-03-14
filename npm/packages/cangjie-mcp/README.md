# cangjie-mcp

Install the `cangjie` CLI from npm.

- `cangjie` executes the Rust CLI directly.
- `cangjie-mcp` forwards to `cangjie mcp`.

The package prefers prebuilt platform binaries and falls back to a local Rust build when the runtime is unsupported or `CANGJIE_MCP_FORCE_BUILD=1`.
