# syntax=docker/dockerfile:1

# ---- Stage 1: compile CLI + server ----
FROM rust:1.85-bookworm AS builder

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev cmake git \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Cache dependency compilation
COPY Cargo.toml Cargo.lock* ./
COPY cangjie-mcp/Cargo.toml cangjie-mcp/Cargo.toml
COPY cangjie-mcp-cli/Cargo.toml cangjie-mcp-cli/Cargo.toml
COPY cangjie-mcp-server/Cargo.toml cangjie-mcp-server/Cargo.toml
RUN mkdir -p cangjie-mcp/src cangjie-mcp-cli/src cangjie-mcp-server/src \
 && echo "" > cangjie-mcp/src/lib.rs \
 && echo "fn main() {}" > cangjie-mcp-cli/src/main.rs \
 && echo "fn main() {}" > cangjie-mcp-server/src/main.rs \
 && cargo build --release -p cangjie-mcp-cli -p cangjie-mcp-server 2>/dev/null || true \
 && rm -rf cangjie-mcp/src cangjie-mcp-cli/src cangjie-mcp-server/src

COPY cangjie-mcp/ cangjie-mcp/
COPY cangjie-mcp-cli/ cangjie-mcp-cli/
COPY cangjie-mcp-server/ cangjie-mcp-server/
RUN cargo build --release -p cangjie-mcp-cli -p cangjie-mcp-server

# ---- Stage 2: build search index using CLI ----
FROM debian:bookworm-slim AS indexer

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates git \
 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/cangjie-mcp /usr/local/bin/cangjie-mcp

ARG CANGJIE_DOCS_VERSION=dev
ARG CANGJIE_DOCS_LANG=zh
ARG CANGJIE_EMBEDDING_TYPE=none

RUN --mount=type=secret,id=OPENAI_API_KEY \
    OPENAI_API_KEY=$(if [ -f /run/secrets/OPENAI_API_KEY ]; then cat /run/secrets/OPENAI_API_KEY; fi) \
    cangjie-mcp \
      --docs-version "${CANGJIE_DOCS_VERSION}" \
      --lang "${CANGJIE_DOCS_LANG}" \
      --embedding "${CANGJIE_EMBEDDING_TYPE}" \
      --data-dir /data \
      index

# ---- Stage 3: minimal runtime with server binary + pre-built index ----
FROM debian:bookworm-slim

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates git \
 && rm -rf /var/lib/apt/lists/* \
 && groupadd --system --gid 999 nonroot \
 && useradd --system --gid 999 --uid 999 --create-home nonroot

COPY --from=builder /app/target/release/cangjie-mcp-server /usr/local/bin/cangjie-mcp-server
COPY --from=indexer --chown=nonroot:nonroot /data /data

ENV CANGJIE_DATA_DIR=/data

USER nonroot

EXPOSE 8765

ENTRYPOINT ["cangjie-mcp-server"]
CMD ["--host", "0.0.0.0"]
