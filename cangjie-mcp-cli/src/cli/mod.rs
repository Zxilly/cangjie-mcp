pub mod commands;
pub mod output;

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use cangjie_core::config::{
    self, DocLang, EmbeddingType, RerankType, Settings, DEFAULT_CHUNK_MAX_SIZE,
    DEFAULT_CHUNK_OVERLAP, DEFAULT_DOCS_VERSION, DEFAULT_HTTP_ENABLE_HTTP2,
    DEFAULT_HTTP_POOL_IDLE_TIMEOUT_SECS, DEFAULT_HTTP_POOL_MAX_IDLE_PER_HOST,
    DEFAULT_HTTP_TCP_KEEPALIVE_SECS, DEFAULT_LOCAL_MODEL, DEFAULT_MAX_PER_FILE,
    DEFAULT_OPENAI_BASE_URL, DEFAULT_OPENAI_MODEL, DEFAULT_RERANK_INITIAL_K, DEFAULT_RERANK_MODEL,
    DEFAULT_RERANK_TOP_K, DEFAULT_RRF_K,
};

pub const DEFAULT_DAEMON_TIMEOUT_MINUTES: u64 = 30;

#[derive(Parser)]
#[command(
    name = "cangjie-mcp",
    about = "Cangjie programming language documentation and code intelligence CLI",
    version
)]
pub struct CangjieArgs {
    /// Log file path
    #[arg(long = "log-file", env = "CANGJIE_LOG_FILE", global = true)]
    pub log_file: Option<PathBuf>,

    /// Enable debug mode
    #[arg(long, env = "CANGJIE_DEBUG", global = true)]
    pub debug: bool,

    /// Daemon idle timeout in minutes
    #[arg(long = "daemon-timeout", env = "CANGJIE_DAEMON_TIMEOUT", default_value_t = DEFAULT_DAEMON_TIMEOUT_MINUTES, hide = true, global = true)]
    pub daemon_timeout: u64,

    #[command(flatten)]
    pub server: ServerOptions,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

// ── Server/index options ───────────────────────────────────────────────

#[derive(Args)]
pub struct ServerOptions {
    /// Documentation version (git tag)
    #[arg(long = "docs-version", short = 'v', env = "CANGJIE_DOCS_VERSION", default_value = DEFAULT_DOCS_VERSION, global = true)]
    pub docs_version: String,

    /// Documentation language (zh/en)
    #[arg(
        long,
        short = 'l',
        env = "CANGJIE_DOCS_LANG",
        default_value = "zh",
        global = true
    )]
    pub lang: DocLang,

    /// Embedding type: none (BM25 only), local, or openai
    #[arg(
        long,
        short = 'e',
        env = "CANGJIE_EMBEDDING_TYPE",
        default_value = "none",
        global = true
    )]
    pub embedding: EmbeddingType,

    /// Local HuggingFace embedding model name
    #[arg(long = "local-model", env = "CANGJIE_LOCAL_MODEL", default_value = DEFAULT_LOCAL_MODEL, global = true)]
    pub local_model: String,

    /// OpenAI API key
    #[arg(long = "openai-api-key", env = "OPENAI_API_KEY", global = true)]
    pub openai_api_key: Option<String>,

    /// OpenAI API base URL
    #[arg(long = "openai-base-url", env = "OPENAI_BASE_URL", default_value = DEFAULT_OPENAI_BASE_URL, global = true)]
    pub openai_base_url: String,

    /// OpenAI embedding model
    #[arg(long = "openai-model", env = "OPENAI_EMBEDDING_MODEL", default_value = DEFAULT_OPENAI_MODEL, global = true)]
    pub openai_model: String,

    /// Rerank type (none/local/openai)
    #[arg(
        long,
        short = 'r',
        env = "CANGJIE_RERANK_TYPE",
        default_value = "none",
        global = true
    )]
    pub rerank: RerankType,

    /// Rerank model name
    #[arg(long = "rerank-model", env = "CANGJIE_RERANK_MODEL", default_value = DEFAULT_RERANK_MODEL, global = true)]
    pub rerank_model: String,

    /// Number of results after reranking
    #[arg(long = "rerank-top-k", env = "CANGJIE_RERANK_TOP_K", default_value_t = DEFAULT_RERANK_TOP_K, global = true)]
    pub rerank_top_k: usize,

    /// Number of candidates before reranking
    #[arg(long = "rerank-initial-k", env = "CANGJIE_RERANK_INITIAL_K", default_value_t = DEFAULT_RERANK_INITIAL_K, global = true)]
    pub rerank_initial_k: usize,

    /// Max chunk size in characters
    #[arg(long = "chunk-size", env = "CANGJIE_CHUNK_MAX_SIZE", default_value_t = DEFAULT_CHUNK_MAX_SIZE, global = true)]
    pub chunk_max_size: usize,

    /// Chunk overlap in characters
    #[arg(long = "chunk-overlap", env = "CANGJIE_CHUNK_OVERLAP", default_value_t = DEFAULT_CHUNK_OVERLAP, global = true)]
    pub chunk_overlap: usize,

    /// Maximum search results per file
    #[arg(long = "max-per-file", env = "CANGJIE_MAX_PER_FILE", default_value_t = DEFAULT_MAX_PER_FILE, global = true)]
    pub max_per_file: usize,

    /// LLM model for generating chunk context summaries
    #[arg(long = "summary-model", env = "CANGJIE_SUMMARY_MODEL", global = true)]
    pub summary_model: Option<String>,

    /// RRF constant k for hybrid search fusion
    #[arg(long = "rrf-k", env = "CANGJIE_RRF_K", default_value_t = DEFAULT_RRF_K, global = true)]
    pub rrf_k: u32,

    /// Data directory path
    #[arg(
        long = "data-dir",
        short = 'd',
        env = "CANGJIE_DATA_DIR",
        global = true
    )]
    pub data_dir: Option<PathBuf>,

    /// URL of a remote cangjie-mcp server to forward queries to
    #[arg(long = "server-url", env = "CANGJIE_SERVER_URL", global = true)]
    pub server_url: Option<String>,

    /// HTTP client pool idle timeout in seconds
    #[arg(long = "http-pool-idle-timeout-secs", env = "CANGJIE_HTTP_POOL_IDLE_TIMEOUT_SECS", default_value_t = DEFAULT_HTTP_POOL_IDLE_TIMEOUT_SECS, global = true)]
    pub http_pool_idle_timeout_secs: u64,

    /// Max idle HTTP connections per host
    #[arg(long = "http-pool-max-idle-per-host", env = "CANGJIE_HTTP_POOL_MAX_IDLE_PER_HOST", default_value_t = DEFAULT_HTTP_POOL_MAX_IDLE_PER_HOST, global = true)]
    pub http_pool_max_idle_per_host: usize,

    /// TCP keepalive for outbound HTTP in seconds
    #[arg(long = "http-tcp-keepalive-secs", env = "CANGJIE_HTTP_TCP_KEEPALIVE_SECS", default_value_t = DEFAULT_HTTP_TCP_KEEPALIVE_SECS, global = true)]
    pub http_tcp_keepalive_secs: u64,

    /// Enable HTTP/2 for outbound HTTP client
    #[arg(long = "http2", env = "CANGJIE_HTTP2", default_value_t = DEFAULT_HTTP_ENABLE_HTTP2, global = true)]
    pub http_enable_http2: bool,
}

impl ServerOptions {
    pub fn to_settings(&self) -> Settings {
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
            chunk_overlap: self.chunk_overlap,
            max_per_file: self.max_per_file,
            summary_model: self.summary_model.clone(),
            data_dir: self
                .data_dir
                .clone()
                .unwrap_or_else(config::get_default_data_dir),
            server_url: self.server_url.clone(),
            openai_api_key: self.openai_api_key.clone(),
            openai_base_url: self.openai_base_url.clone(),
            openai_model: self.openai_model.clone(),
            http_pool_idle_timeout_secs: self.http_pool_idle_timeout_secs,
            http_pool_max_idle_per_host: self.http_pool_max_idle_per_host,
            http_tcp_keepalive_secs: self.http_tcp_keepalive_secs,
            http_enable_http2: self.http_enable_http2,
            ..Settings::default()
        }
    }
}

// ── Commands ─────────────────────────────────────────────────────────────

#[derive(Subcommand)]
pub enum Commands {
    /// Search Cangjie documentation
    Query {
        /// Search query
        query: String,
        /// Filter by category
        #[arg(long, short = 'c')]
        category: Option<String>,
        /// Number of results (default: 5, max: 20)
        #[arg(long, short = 'k', default_value_t = 5)]
        top_k: usize,
        /// Offset for pagination
        #[arg(long, default_value_t = 0)]
        offset: usize,
        /// Filter by stdlib package name
        #[arg(long)]
        package: Option<String>,
    },
    /// Get documentation for a topic (with pagination)
    Topic {
        /// Topic name
        name: String,
        /// Optional category filter
        #[arg(long, short = 'c')]
        category: Option<String>,
        /// Byte offset to start reading from
        #[arg(long, default_value_t = 0)]
        offset: usize,
        /// Maximum number of bytes to return
        #[arg(long, default_value_t = 10000)]
        max_length: usize,
    },
    /// List available documentation topics
    Topics {
        /// Optional category filter
        #[arg(long, short = 'c')]
        category: Option<String>,
    },
    /// LSP code intelligence operations
    Lsp {
        #[command(subcommand)]
        operation: LspCommand,
    },
    /// Internal: run as daemon (hidden)
    #[command(hide = true)]
    Serve,
    /// Build the search index
    Index,
    /// Daemon management
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    /// Configuration file management
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand)]
pub enum LspCommand {
    /// Go to definition
    Definition {
        /// Source file path
        file: String,
        /// Symbol name to look up
        #[arg(long)]
        symbol: Option<String>,
        /// Line number (1-based)
        #[arg(long)]
        line: Option<u32>,
        /// Character position (1-based)
        #[arg(long, alias = "char")]
        character: Option<u32>,
    },
    /// Find references
    References {
        file: String,
        #[arg(long)]
        symbol: Option<String>,
        #[arg(long)]
        line: Option<u32>,
        #[arg(long, alias = "char")]
        character: Option<u32>,
    },
    /// Hover information
    Hover {
        file: String,
        #[arg(long)]
        symbol: Option<String>,
        #[arg(long)]
        line: Option<u32>,
        #[arg(long, alias = "char")]
        character: Option<u32>,
    },
    /// List document symbols
    Symbols { file: String },
    /// Get file diagnostics
    Diagnostics { file: String },
    /// Search workspace symbols
    WorkspaceSymbol {
        /// Search query
        query: String,
    },
    /// Get completions at position
    Completion {
        file: String,
        /// Line number (1-based)
        #[arg(long)]
        line: u32,
        /// Character position (1-based)
        #[arg(long, alias = "char")]
        character: u32,
    },
    /// Rename symbol
    Rename {
        file: String,
        #[arg(long)]
        symbol: String,
        /// New name for the symbol
        #[arg(long)]
        new_name: String,
    },
    /// Find incoming calls
    IncomingCalls {
        file: String,
        #[arg(long)]
        symbol: Option<String>,
        #[arg(long)]
        line: Option<u32>,
        #[arg(long, alias = "char")]
        character: Option<u32>,
    },
    /// Find outgoing calls
    OutgoingCalls {
        file: String,
        #[arg(long)]
        symbol: Option<String>,
        #[arg(long)]
        line: Option<u32>,
        #[arg(long, alias = "char")]
        character: Option<u32>,
    },
    /// Find type supertypes
    TypeSupertypes {
        file: String,
        #[arg(long)]
        symbol: Option<String>,
        #[arg(long)]
        line: Option<u32>,
        #[arg(long, alias = "char")]
        character: Option<u32>,
    },
    /// Find type subtypes
    TypeSubtypes {
        file: String,
        #[arg(long)]
        symbol: Option<String>,
        #[arg(long)]
        line: Option<u32>,
        #[arg(long, alias = "char")]
        character: Option<u32>,
    },
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Show config file path
    Path,
    /// Create a default config file with all options commented out
    Init,
}

#[derive(Subcommand)]
pub enum DaemonAction {
    /// Stop the daemon
    Stop,
    /// Show daemon status
    Status,
    /// Show daemon logs
    Logs {
        /// Number of lines to show from the end
        #[arg(long, short = 'n', default_value_t = 50)]
        tail: usize,
        /// Follow log output
        #[arg(long, short = 'f')]
        follow: bool,
    },
}
