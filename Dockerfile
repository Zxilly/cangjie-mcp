# syntax=docker/dockerfile:1
FROM ghcr.io/astral-sh/uv:python3.13-bookworm-slim

ARG OPENAI_BASE_URL=https://api.siliconflow.cn/v1
ARG OPENAI_EMBEDDING_MODEL=BAAI/bge-m3
ARG CANGJIE_DOCS_VERSION=dev
ARG CANGJIE_DOCS_LANG=zh
ARG CANGJIE_RERANK_TYPE=openai
ARG CANGJIE_RERANK_MODEL=BAAI/bge-reranker-v2-m3

RUN groupadd --system --gid 999 nonroot \
 && useradd --system --gid 999 --uid 999 --create-home nonroot

WORKDIR /app

ENV UV_COMPILE_BYTECODE=1
ENV UV_LINK_MODE=copy
ENV UV_NO_DEV=1
ENV UV_TOOL_BIN_DIR=/usr/local/bin

ENV CANGJIE_EMBEDDING_TYPE=openai \
    OPENAI_BASE_URL=${OPENAI_BASE_URL} \
    OPENAI_EMBEDDING_MODEL=${OPENAI_EMBEDDING_MODEL} \
    CANGJIE_DOCS_VERSION=${CANGJIE_DOCS_VERSION} \
    CANGJIE_DOCS_LANG=${CANGJIE_DOCS_LANG} \
    CANGJIE_RERANK_TYPE=${CANGJIE_RERANK_TYPE} \
    CANGJIE_RERANK_MODEL=${CANGJIE_RERANK_MODEL} \
    CANGJIE_DATA_DIR=/data

RUN apt-get update \
 && apt-get install -y --no-install-recommends git \
 && rm -rf /var/lib/apt/lists/*

# Install dependencies first for optimal layer caching
RUN --mount=type=cache,target=/root/.cache/uv \
    --mount=type=bind,source=uv.lock,target=uv.lock \
    --mount=type=bind,source=pyproject.toml,target=pyproject.toml \
    --mount=type=bind,source=.python-version,target=.python-version \
    uv sync --locked --no-install-project

COPY . /app
RUN --mount=type=cache,target=/root/.cache/uv \
    uv sync --locked

ENV PATH="/app/.venv/bin:$PATH"

# Pre-build the search index using a build-time secret.
# Build: docker build --secret id=OPENAI_API_KEY,env=OPENAI_API_KEY .
RUN --mount=type=secret,id=OPENAI_API_KEY python scripts/build_index.py

RUN chown -R nonroot:nonroot /data

ENTRYPOINT []

USER nonroot

EXPOSE 8765

CMD ["cangjie-mcp", "server", "--host", "0.0.0.0"]
