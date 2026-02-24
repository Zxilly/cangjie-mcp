use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use tracing::info;

use cangjie_mcp::config::{
    self, DocLang, EmbeddingType, PrebuiltMode, RerankType, Settings, DEFAULT_CHUNK_MAX_SIZE,
    DEFAULT_DOCS_VERSION, DEFAULT_HTTP_ENABLE_HTTP2, DEFAULT_HTTP_POOL_IDLE_TIMEOUT_SECS,
    DEFAULT_HTTP_POOL_MAX_IDLE_PER_HOST, DEFAULT_HTTP_TCP_KEEPALIVE_SECS, DEFAULT_LOCAL_MODEL,
    DEFAULT_OPENAI_BASE_URL, DEFAULT_OPENAI_MODEL, DEFAULT_RERANK_INITIAL_K, DEFAULT_RERANK_MODEL,
    DEFAULT_RERANK_TOP_K, DEFAULT_RRF_K, DEFAULT_SERVER_ENABLE_HTTP2, DEFAULT_SERVER_HOST,
    DEFAULT_SERVER_PORT,
};
use cangjie_mcp::indexer::document::source::GitDocumentSource;
use cangjie_mcp::indexer::search::LocalSearchIndex;
use cangjie_mcp::indexer::IndexMetadata;
use cangjie_mcp::server::http::create_http_app;

#[derive(Parser)]
#[command(
    name = "cangjie-mcp-server",
    about = "HTTP query server for Cangjie programming language documentation",
    version
)]
struct Cli {
    /// Documentation version (git tag)
    #[arg(long = "docs-version", short = 'v', env = "CANGJIE_DOCS_VERSION", default_value = DEFAULT_DOCS_VERSION)]
    docs_version: String,

    /// Documentation language (zh/en)
    #[arg(long, short = 'l', env = "CANGJIE_DOCS_LANG", value_enum, default_value_t = DocLang::Zh)]
    lang: DocLang,

    /// Embedding type: none (BM25 only), local, or openai
    #[arg(long, short = 'e', env = "CANGJIE_EMBEDDING_TYPE", value_enum, default_value_t = EmbeddingType::None)]
    embedding: EmbeddingType,

    /// Local HuggingFace embedding model name
    #[arg(long = "local-model", env = "CANGJIE_LOCAL_MODEL", default_value = DEFAULT_LOCAL_MODEL)]
    local_model: String,

    /// OpenAI API key
    #[arg(long = "openai-api-key", env = "OPENAI_API_KEY")]
    openai_api_key: Option<String>,

    /// OpenAI API base URL
    #[arg(long = "openai-base-url", env = "OPENAI_BASE_URL", default_value = DEFAULT_OPENAI_BASE_URL)]
    openai_base_url: String,

    /// OpenAI embedding model
    #[arg(long = "openai-model", env = "OPENAI_EMBEDDING_MODEL", default_value = DEFAULT_OPENAI_MODEL)]
    openai_model: String,

    /// Rerank type (none/local/openai)
    #[arg(long, short = 'r', env = "CANGJIE_RERANK_TYPE", value_enum, default_value_t = RerankType::None)]
    rerank: RerankType,

    /// Rerank model name
    #[arg(long = "rerank-model", env = "CANGJIE_RERANK_MODEL", default_value = DEFAULT_RERANK_MODEL)]
    rerank_model: String,

    /// Number of results after reranking
    #[arg(long = "rerank-top-k", env = "CANGJIE_RERANK_TOP_K", default_value_t = DEFAULT_RERANK_TOP_K)]
    rerank_top_k: usize,

    /// Number of candidates before reranking
    #[arg(long = "rerank-initial-k", env = "CANGJIE_RERANK_INITIAL_K", default_value_t = DEFAULT_RERANK_INITIAL_K)]
    rerank_initial_k: usize,

    /// Max chunk size in characters
    #[arg(long = "chunk-size", env = "CANGJIE_CHUNK_MAX_SIZE", default_value_t = DEFAULT_CHUNK_MAX_SIZE)]
    chunk_max_size: usize,

    /// RRF constant k for hybrid search fusion
    #[arg(long = "rrf-k", env = "CANGJIE_RRF_K", default_value_t = DEFAULT_RRF_K)]
    rrf_k: u32,

    /// Data directory path
    #[arg(long = "data-dir", short = 'd', env = "CANGJIE_DATA_DIR")]
    data_dir: Option<PathBuf>,

    /// Host to bind the HTTP server to
    #[arg(long, env = "CANGJIE_SERVER_HOST", default_value = DEFAULT_SERVER_HOST)]
    host: String,

    /// Port to bind the HTTP server to
    #[arg(long, short = 'p', env = "CANGJIE_SERVER_PORT", default_value_t = DEFAULT_SERVER_PORT)]
    port: u16,

    /// HTTP client pool idle timeout in seconds
    #[arg(long = "http-pool-idle-timeout-secs", env = "CANGJIE_HTTP_POOL_IDLE_TIMEOUT_SECS", default_value_t = DEFAULT_HTTP_POOL_IDLE_TIMEOUT_SECS)]
    http_pool_idle_timeout_secs: u64,

    /// Max idle HTTP connections per host
    #[arg(long = "http-pool-max-idle-per-host", env = "CANGJIE_HTTP_POOL_MAX_IDLE_PER_HOST", default_value_t = DEFAULT_HTTP_POOL_MAX_IDLE_PER_HOST)]
    http_pool_max_idle_per_host: usize,

    /// TCP keepalive for outbound HTTP in seconds
    #[arg(long = "http-tcp-keepalive-secs", env = "CANGJIE_HTTP_TCP_KEEPALIVE_SECS", default_value_t = DEFAULT_HTTP_TCP_KEEPALIVE_SECS)]
    http_tcp_keepalive_secs: u64,

    /// Enable HTTP/2 for outbound HTTP client
    #[arg(long = "http2", env = "CANGJIE_HTTP2", default_value_t = DEFAULT_HTTP_ENABLE_HTTP2)]
    http_enable_http2: bool,

    /// Enable HTTP/2 for the HTTP server
    #[arg(long = "server-http2", env = "CANGJIE_SERVER_HTTP2", default_value_t = DEFAULT_SERVER_ENABLE_HTTP2)]
    server_enable_http2: bool,

    /// Log file path
    #[arg(long = "log-file", env = "CANGJIE_LOG_FILE")]
    log_file: Option<PathBuf>,

    /// Enable debug mode
    #[arg(long, env = "CANGJIE_DEBUG")]
    debug: bool,

    /// Use pre-built index, optionally specifying a version (for Docker runtime)
    #[arg(long, env = "CANGJIE_PREBUILT", num_args = 0..=1, default_missing_value = "true", value_name = "VERSION")]
    prebuilt: Option<String>,
}

impl Cli {
    fn to_settings(&self) -> Settings {
        Settings {
            docs_version: self.docs_version.clone(),
            docs_lang: self.lang,
            embedding_type: self.embedding,
            local_model: self.local_model.clone(),
            rerank_type: self.rerank,
            rerank_model: self.rerank_model.clone(),
            rerank_top_k: self.rerank_top_k,
            rerank_initial_k: self.rerank_initial_k,
            rrf_k: self.rrf_k,
            chunk_max_size: self.chunk_max_size,
            data_dir: self
                .data_dir
                .clone()
                .unwrap_or_else(config::get_default_data_dir),
            openai_api_key: self.openai_api_key.clone(),
            openai_base_url: self.openai_base_url.clone(),
            openai_model: self.openai_model.clone(),
            http_pool_idle_timeout_secs: self.http_pool_idle_timeout_secs,
            http_pool_max_idle_per_host: self.http_pool_max_idle_per_host,
            http_tcp_keepalive_secs: self.http_tcp_keepalive_secs,
            http_enable_http2: self.http_enable_http2,
            server_enable_http2: self.server_enable_http2,
            prebuilt: match &self.prebuilt {
                None => PrebuiltMode::Off,
                Some(v) if v == "true" || v.is_empty() => PrebuiltMode::Auto,
                Some(v) => PrebuiltMode::Version(v.clone()),
            },
            ..Settings::default()
        }
    }
}

fn setup_logging(log_file: Option<&PathBuf>, debug: bool) {
    use tracing_subscriber::EnvFilter;

    let filter = if debug {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("info")
    };

    if let Some(log_path) = log_file {
        if let Some(parent) = log_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
            .expect("Failed to open log file");

        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::sync::Mutex::new(file))
            .with_ansi(false)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .init();
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    setup_logging(cli.log_file.as_ref(), cli.debug);

    let settings = cli.to_settings();

    info!(
        "Initializing index (version={}, lang={})...",
        settings.docs_version, settings.docs_lang
    );

    let mut search_index = LocalSearchIndex::new(settings.clone()).await;
    let index_info = search_index.init().await?;

    config::log_startup_info(&settings, &index_info);

    let metadata_path = index_info.index_dir().join("index_metadata.json");
    let index_metadata: IndexMetadata =
        serde_json::from_str(&std::fs::read_to_string(&metadata_path)?)?;

    let doc_source = GitDocumentSource::new(settings.docs_repo_dir(), index_info.lang)?;

    let app = create_http_app(search_index, Box::new(doc_source), index_metadata).await;

    let bind_addr = format!("{}:{}", cli.host, cli.port);
    info!("Starting HTTP server on {bind_addr}...");
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    if cli.server_enable_http2 {
        info!("HTTP/2 enabled on server.");
    }
    axum::serve(listener, app).await?;

    Ok(())
}
