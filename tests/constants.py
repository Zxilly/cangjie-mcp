"""Shared test constants.

This file contains constants used across unit and integration tests.
Version numbers are locked to ensure reproducible test results.

These constants can be overridden via environment variables:
- CANGJIE_TEST_DOCS_VERSION: Override docs version
- CANGJIE_TEST_LOCAL_MODEL: Override local embedding model
- CANGJIE_TEST_RERANKER_MODEL: Override reranker model (default is large;
  set to e.g. "cross-encoder/ms-marco-MiniLM-L-2-v2" for faster CI runs)
"""

import os

# Locked Cangjie documentation version for testing
# This should match a valid git tag from https://gitcode.com/Cangjie/cangjie_docs
# Available stable tags: v1.0.2, v1.0.5, v1.0.6, v1.0.7
# Update this when testing against a new docs version
CANGJIE_DOCS_VERSION = os.environ.get("CANGJIE_TEST_DOCS_VERSION", "v1.0.7")

# Local embedding model used for testing
# This model is used for offline testing without external API calls
CANGJIE_LOCAL_MODEL = os.environ.get("CANGJIE_TEST_LOCAL_MODEL", "paraphrase-multilingual-MiniLM-L12-v2")

# Reranker model used for testing
# Default BAAI/bge-reranker-v2-m3 is ~567 MB and takes ~15 s to load on CPU.
# For faster test runs, set to a smaller model such as
# "cross-encoder/ms-marco-MiniLM-L-2-v2" (~24 MB, loads in ~1 s).
CANGJIE_RERANKER_MODEL = os.environ.get("CANGJIE_TEST_RERANKER_MODEL", "BAAI/bge-reranker-v2-m3")
