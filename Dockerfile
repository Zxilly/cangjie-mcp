# syntax=docker/dockerfile:1

# ---- Stage 1: system deps ----
FROM rust:1-bookworm AS base

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev cmake git protobuf-compiler libprotobuf-dev mold clang \
 && rm -rf /var/lib/apt/lists/*

# ---- Stage 1b: install cargo-chef (separate layer) ----
FROM base AS chef
RUN cargo install cargo-chef
WORKDIR /app

# ---- Stage 2: analyze dependencies ----
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ---- Stage 3: compile CLI + server ----
FROM chef AS builder

ENV RUSTFLAGS="-C linker=clang -C link-arg=-fuse-ld=mold"

COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo chef cook --release --recipe-path recipe.json

COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release -p cangjie-mcp-cli -p cangjie-mcp-server \
 && cp target/release/cangjie-mcp /usr/local/bin/ \
 && cp target/release/cangjie-mcp-server /usr/local/bin/

# ---- Stage 4: build search index (OpenAI embeddings required) ----
FROM debian:bookworm-slim AS indexer

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/local/bin/cangjie-mcp /usr/local/bin/cangjie-mcp

ARG CANGJIE_DOCS_VERSION=dev
ARG CANGJIE_DOCS_LANG=zh
ARG OPENAI_EMBEDDING_MODEL=BAAI/bge-m3
ARG OPENAI_BASE_URL=https://api.siliconflow.cn/v1

RUN --mount=type=secret,id=OPENAI_API_KEY \
    if [ ! -f /run/secrets/OPENAI_API_KEY ]; then \
        echo "ERROR: OPENAI_API_KEY secret is required for building the OpenAI embedding index." >&2; \
        echo "  Pass it via: docker build --secret id=OPENAI_API_KEY,env=OPENAI_API_KEY ..." >&2; \
        exit 1; \
    fi \
 && OPENAI_API_KEY=$(cat /run/secrets/OPENAI_API_KEY) \
    OPENAI_EMBEDDING_MODEL="${OPENAI_EMBEDDING_MODEL}" \
    OPENAI_BASE_URL="${OPENAI_BASE_URL}" \
    cangjie-mcp \
      --docs-version "${CANGJIE_DOCS_VERSION}" \
      --lang "${CANGJIE_DOCS_LANG}" \
      --embedding openai \
      --data-dir /data \
      index

# ---- Stage 5: minimal runtime with server binary + pre-built index ----
FROM debian:bookworm-slim

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && groupadd --system --gid 999 nonroot \
 && useradd --system --gid 999 --uid 999 --create-home nonroot

COPY --from=builder /usr/local/bin/cangjie-mcp-server /usr/local/bin/cangjie-mcp-server
COPY --from=indexer --chown=nonroot:nonroot /data /data
COPY scripts/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

# Re-declare build ARGs so they carry into ENV defaults
ARG CANGJIE_DOCS_VERSION=dev
ARG CANGJIE_DOCS_LANG=zh
ARG OPENAI_EMBEDDING_MODEL=BAAI/bge-m3
ARG OPENAI_BASE_URL=https://api.siliconflow.cn/v1

# Bake build-time settings as runtime ENV defaults
ENV CANGJIE_DATA_DIR=/data
ENV CANGJIE_EMBEDDING_TYPE=openai
ENV CANGJIE_RERANK_TYPE=openai
ENV CANGJIE_DOCS_VERSION=${CANGJIE_DOCS_VERSION}
ENV CANGJIE_DOCS_LANG=${CANGJIE_DOCS_LANG}
ENV OPENAI_EMBEDDING_MODEL=${OPENAI_EMBEDDING_MODEL}
ENV OPENAI_BASE_URL=${OPENAI_BASE_URL}
# Internal: records the build-time model for entrypoint validation
ENV CANGJIE_BUILD_EMBEDDING_MODEL=${OPENAI_EMBEDDING_MODEL}

USER nonroot

EXPOSE 8765

ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
CMD ["cangjie-mcp-server", "--prebuilt", "--host", "0.0.0.0"]
