"""CLI for Cangjie MCP server.

All CLI options can be configured via environment variables.
Run `cangjie-mcp --help` to see all options and their environment variables.

Environment variable naming:
- CANGJIE_* prefix for most options
- OPENAI_* prefix for OpenAI-related options
"""

from typing import Annotated

import typer

from cangjie_mcp import __version__
from cangjie_mcp.cli_args import (
    ChunkSizeOption,
    DataDirOption,
    DebugOption,
    DocsVersionOption,
    EmbeddingOption,
    HostOption,
    LangOption,
    LocalModelOption,
    LogFileOption,
    OpenAIApiKeyOption,
    OpenAIBaseUrlOption,
    OpenAIModelOption,
    PortOption,
    RerankInitialKOption,
    RerankModelOption,
    RerankOption,
    RerankTopKOption,
    ServerUrlOption,
    validate_embedding_type,
    validate_lang,
    validate_rerank_type,
)
from cangjie_mcp.config import Settings, set_settings
from cangjie_mcp.defaults import (
    DEFAULT_CHUNK_MAX_SIZE,
    DEFAULT_DOCS_LANG,
    DEFAULT_DOCS_VERSION,
    DEFAULT_EMBEDDING_TYPE,
    DEFAULT_LOCAL_MODEL,
    DEFAULT_OPENAI_BASE_URL,
    DEFAULT_OPENAI_MODEL,
    DEFAULT_RERANK_INITIAL_K,
    DEFAULT_RERANK_MODEL,
    DEFAULT_RERANK_TOP_K,
    DEFAULT_RERANK_TYPE,
    DEFAULT_SERVER_HOST,
    DEFAULT_SERVER_PORT,
    get_default_data_dir,
)
from cangjie_mcp.utils import logger, setup_logging


def _version_callback(value: bool) -> None:
    """Print version and exit."""
    if value:
        typer.echo(f"cangjie-mcp {__version__}")
        raise typer.Exit()


# Root app
app = typer.Typer(
    name="cangjie-mcp",
    help="MCP server for Cangjie programming language",
    invoke_without_command=True,
)


@app.callback(invoke_without_command=True)
def main(
    ctx: typer.Context,
    _version: Annotated[
        bool,
        typer.Option(
            "--version",
            "-v",
            help="Show version and exit",
            callback=_version_callback,
            is_eager=True,
        ),
    ] = False,
    # Docs options - using shared type aliases
    docs_version: DocsVersionOption = DEFAULT_DOCS_VERSION,
    lang: LangOption = DEFAULT_DOCS_LANG,
    embedding: EmbeddingOption = DEFAULT_EMBEDDING_TYPE,
    local_model: LocalModelOption = DEFAULT_LOCAL_MODEL,
    openai_api_key: OpenAIApiKeyOption = None,
    openai_base_url: OpenAIBaseUrlOption = DEFAULT_OPENAI_BASE_URL,
    openai_model: OpenAIModelOption = DEFAULT_OPENAI_MODEL,
    rerank: RerankOption = DEFAULT_RERANK_TYPE,
    rerank_model: RerankModelOption = DEFAULT_RERANK_MODEL,
    rerank_top_k: RerankTopKOption = DEFAULT_RERANK_TOP_K,
    rerank_initial_k: RerankInitialKOption = DEFAULT_RERANK_INITIAL_K,
    chunk_max_size: ChunkSizeOption = DEFAULT_CHUNK_MAX_SIZE,
    data_dir: DataDirOption = None,
    server_url: ServerUrlOption = None,
    log_file: LogFileOption = None,
    debug: DebugOption = False,
) -> None:
    """Start the MCP server."""
    # If a subcommand is invoked, let it handle execution
    if ctx.invoked_subcommand is not None:
        return

    # Set up logging early
    setup_logging(log_file, debug)

    if server_url:
        logger.info("Using remote server at %s — local index options are ignored.", server_url)

    # Validate and build settings
    settings = Settings(
        docs_version=docs_version,
        docs_lang=validate_lang(lang),  # type: ignore[arg-type]
        embedding_type=validate_embedding_type(embedding),  # type: ignore[arg-type]
        local_model=local_model,
        openai_api_key=openai_api_key,
        openai_base_url=openai_base_url,
        openai_model=openai_model,
        rerank_type=validate_rerank_type(rerank),  # type: ignore[arg-type]
        rerank_model=rerank_model,
        rerank_top_k=rerank_top_k,
        rerank_initial_k=rerank_initial_k,
        chunk_max_size=chunk_max_size,
        data_dir=data_dir if data_dir else get_default_data_dir(),
        server_url=server_url,
    )
    set_settings(settings)

    # Create and run server
    from cangjie_mcp.server.factory import create_mcp_server

    mcp = create_mcp_server(settings)
    logger.info("Starting MCP server...")
    mcp.run(transport="stdio")


@app.command("server")
def server_command(
    # Index options (same as main)
    docs_version: DocsVersionOption = DEFAULT_DOCS_VERSION,
    lang: LangOption = DEFAULT_DOCS_LANG,
    embedding: EmbeddingOption = DEFAULT_EMBEDDING_TYPE,
    local_model: LocalModelOption = DEFAULT_LOCAL_MODEL,
    openai_api_key: OpenAIApiKeyOption = None,
    openai_base_url: OpenAIBaseUrlOption = DEFAULT_OPENAI_BASE_URL,
    openai_model: OpenAIModelOption = DEFAULT_OPENAI_MODEL,
    rerank: RerankOption = DEFAULT_RERANK_TYPE,
    rerank_model: RerankModelOption = DEFAULT_RERANK_MODEL,
    rerank_top_k: RerankTopKOption = DEFAULT_RERANK_TOP_K,
    rerank_initial_k: RerankInitialKOption = DEFAULT_RERANK_INITIAL_K,
    chunk_max_size: ChunkSizeOption = DEFAULT_CHUNK_MAX_SIZE,
    data_dir: DataDirOption = None,
    log_file: LogFileOption = None,
    debug: DebugOption = False,
    # Server-specific options
    host: HostOption = DEFAULT_SERVER_HOST,
    port: PortOption = DEFAULT_SERVER_PORT,
) -> None:
    """Start the HTTP query server.

    Loads the embedding model and ChromaDB index, then serves search
    queries over HTTP. Remote MCP clients can connect using --server-url.
    """
    setup_logging(log_file, debug)

    settings = Settings(
        docs_version=docs_version,
        docs_lang=validate_lang(lang),  # type: ignore[arg-type]
        embedding_type=validate_embedding_type(embedding),  # type: ignore[arg-type]
        local_model=local_model,
        openai_api_key=openai_api_key,
        openai_base_url=openai_base_url,
        openai_model=openai_model,
        rerank_type=validate_rerank_type(rerank),  # type: ignore[arg-type]
        rerank_model=rerank_model,
        rerank_top_k=rerank_top_k,
        rerank_initial_k=rerank_initial_k,
        chunk_max_size=chunk_max_size,
        data_dir=data_dir if data_dir else get_default_data_dir(),
    )
    set_settings(settings)

    from cangjie_mcp.indexer.search_index import LocalSearchIndex
    from cangjie_mcp.indexer.store import METADATA_FILE, IndexMetadata
    from cangjie_mcp.server.http import create_http_app

    # Initialize the local search index
    typer.echo(f"Initializing index (version={settings.docs_version}, lang={settings.docs_lang})...")
    search_index = LocalSearchIndex(settings)
    index_info = search_index.init()
    typer.echo(f"Index ready: version={index_info.version}, lang={index_info.lang}")

    # Load index metadata for the /info endpoint
    metadata_path = index_info.chroma_db_dir / METADATA_FILE
    index_metadata = IndexMetadata.model_validate_json(metadata_path.read_text(encoding="utf-8"))

    # Create document source
    from cangjie_mcp.indexer.document_source import GitDocumentSource
    from cangjie_mcp.repo.git_manager import GitManager

    git_mgr = GitManager(settings.docs_repo_dir)
    if not git_mgr.is_cloned() or git_mgr.repo is None:
        raise RuntimeError(
            f"Documentation repository not found at {settings.docs_repo_dir}. "
            "The index was built but the git repo is missing."
        )

    doc_source = GitDocumentSource(
        repo=git_mgr.repo,
        version=index_info.version,
        lang=index_info.lang,
    )

    # Create and run HTTP app
    http_app = create_http_app(search_index, doc_source, index_metadata)

    typer.echo(f"Starting HTTP server on {host}:{port}...")
    import uvicorn

    uvicorn.run(http_app, host=host, port=port)


if __name__ == "__main__":
    app()
