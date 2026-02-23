#!/bin/sh
set -e

# --- Validate OPENAI_API_KEY ---
if [ -z "$OPENAI_API_KEY" ]; then
    echo "ERROR: OPENAI_API_KEY is required but not set." >&2
    echo "  Pass it via: docker run -e OPENAI_API_KEY=sk-xxx ..." >&2
    exit 1
fi

# --- Ensure embedding type matches pre-built index ---
if [ "$CANGJIE_EMBEDDING_TYPE" != "openai" ]; then
    echo "ERROR: CANGJIE_EMBEDDING_TYPE must be 'openai' to match the pre-built index (got '${CANGJIE_EMBEDDING_TYPE}')." >&2
    exit 1
fi

# --- Ensure reranker is enabled ---
if [ "$CANGJIE_RERANK_TYPE" != "openai" ]; then
    echo "ERROR: CANGJIE_RERANK_TYPE must be 'openai' (got '${CANGJIE_RERANK_TYPE}')." >&2
    exit 1
fi

# --- Validate embedding model matches build-time value ---
BUILD_MODEL_FILE="/data/.build_embedding_model"
CANGJIE_BUILD_EMBEDDING_MODEL=$(cat "$BUILD_MODEL_FILE")
if [ "$OPENAI_EMBEDDING_MODEL" != "$CANGJIE_BUILD_EMBEDDING_MODEL" ]; then
    echo "ERROR: OPENAI_EMBEDDING_MODEL='${OPENAI_EMBEDDING_MODEL}' does not match the pre-built index model '${CANGJIE_BUILD_EMBEDDING_MODEL}'." >&2
    echo "  Remove OPENAI_EMBEDDING_MODEL override or rebuild the image with the desired model." >&2
    exit 1
fi

exec "$@"
