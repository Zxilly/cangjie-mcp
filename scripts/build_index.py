"""Pre-build the search index inside Docker.

Called during `docker build` with OPENAI_API_KEY mounted as a build secret.
All other configuration is read from environment variables set in the Dockerfile.
"""

import os
from pathlib import Path

from cangjie_mcp.config import Settings, set_settings
from cangjie_mcp.indexer.initializer import initialize_and_index

with Path("/run/secrets/OPENAI_API_KEY").open() as f:
    api_key = f.read().strip()

settings = Settings(
    embedding_type="openai",
    openai_api_key=api_key,
    openai_base_url=os.environ["OPENAI_BASE_URL"],
    openai_model=os.environ["OPENAI_EMBEDDING_MODEL"],
    docs_version=os.environ.get("CANGJIE_DOCS_VERSION", "latest"),
    docs_lang=os.environ.get("CANGJIE_DOCS_LANG", "zh"),  # type: ignore[arg-type]
    data_dir=Path(os.environ.get("CANGJIE_DATA_DIR", "/data")),
)
set_settings(settings)

index_info = initialize_and_index(settings)
print(f"Index built: version={index_info.version}, lang={index_info.lang}, embedding={index_info.embedding_model_name}")
