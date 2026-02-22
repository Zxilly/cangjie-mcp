# syntax=docker/dockerfile:1

# ---- builder stage: compile Rust binary and pre-build index ----
FROM rust:1.85-bookworm AS builder

ARG OPENAI_BASE_URL=https://api.siliconflow.cn/v1
ARG OPENAI_EMBEDDING_MODEL=BAAI/bge-m3
ARG CANGJIE_DOCS_VERSION=dev
ARG CANGJIE_DOCS_LANG=zh
ARG CANGJIE_RERANK_TYPE=openai
ARG CANGJIE_RERANK_MODEL=BAAI/bge-reranker-v2-m3

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev cmake git \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Cache dependency compilation
COPY Cargo.toml Cargo.lock* ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && echo "" > src/lib.rs \
 && cargo build --release 2>/dev/null || true \
 && rm -rf src

# Build the actual binary
COPY src/ src/
RUN cargo build --release

# Pre-build the search index
ENV CANGJIE_EMBEDDING_TYPE=openai \
    OPENAI_BASE_URL=${OPENAI_BASE_URL} \
    OPENAI_EMBEDDING_MODEL=${OPENAI_EMBEDDING_MODEL} \
    CANGJIE_DOCS_VERSION=${CANGJIE_DOCS_VERSION} \
    CANGJIE_DOCS_LANG=${CANGJIE_DOCS_LANG} \
    CANGJIE_RERANK_TYPE=${CANGJIE_RERANK_TYPE} \
    CANGJIE_RERANK_MODEL=${CANGJIE_RERANK_MODEL} \
    CANGJIE_DATA_DIR=/data

# Pre-build index if OPENAI_API_KEY is provided
RUN --mount=type=secret,id=OPENAI_API_KEY \
    if [ -f /run/secrets/OPENAI_API_KEY ]; then \
      export OPENAI_API_KEY=$(cat /run/secrets/OPENAI_API_KEY) && \
      ./target/release/cangjie-mcp --help || true; \
    fi


# ---- runtime stage: minimal image ----
FROM debian:bookworm-slim

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates git \
 && rm -rf /var/lib/apt/lists/* \
 && groupadd --system --gid 999 nonroot \
 && useradd --system --gid 999 --uid 999 --create-home nonroot

WORKDIR /app

ARG OPENAI_BASE_URL=https://api.siliconflow.cn/v1
ARG OPENAI_EMBEDDING_MODEL=BAAI/bge-m3
ARG CANGJIE_DOCS_VERSION=dev
ARG CANGJIE_DOCS_LANG=zh
ARG CANGJIE_RERANK_TYPE=openai
ARG CANGJIE_RERANK_MODEL=BAAI/bge-reranker-v2-m3

ENV CANGJIE_EMBEDDING_TYPE=openai \
    OPENAI_BASE_URL=${OPENAI_BASE_URL} \
    OPENAI_EMBEDDING_MODEL=${OPENAI_EMBEDDING_MODEL} \
    CANGJIE_DOCS_VERSION=${CANGJIE_DOCS_VERSION} \
    CANGJIE_DOCS_LANG=${CANGJIE_DOCS_LANG} \
    CANGJIE_RERANK_TYPE=${CANGJIE_RERANK_TYPE} \
    CANGJIE_RERANK_MODEL=${CANGJIE_RERANK_MODEL} \
    CANGJIE_DATA_DIR=/data

# Copy binary and pre-built data
COPY --from=builder --chown=nonroot:nonroot /app/target/release/cangjie-mcp /usr/local/bin/cangjie-mcp
COPY --from=builder --chown=nonroot:nonroot /data /data

USER nonroot

EXPOSE 8765

CMD ["cangjie-mcp", "server", "--host", "0.0.0.0"]
