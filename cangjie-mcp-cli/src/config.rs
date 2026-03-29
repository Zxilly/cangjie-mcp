use std::path::PathBuf;

use cangjie_core::config::*;
use serde::{Deserialize, Serialize};

/// Config file location:
/// - Linux:   $XDG_CONFIG_HOME/cangjie/config.toml  or  ~/.config/cangjie/config.toml
/// - macOS:   ~/Library/Application Support/cangjie/config.toml
/// - Windows: %APPDATA%\cangjie\config.toml
pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("cangjie")
}

pub fn config_file() -> PathBuf {
    config_dir().join("config.toml")
}

/// TOML config file structure. All fields are optional — only set fields override defaults.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FileConfig {
    pub docs_version: Option<String>,
    pub lang: Option<String>,
    pub embedding: Option<String>,
    pub local_model: Option<String>,
    pub openai_api_key: Option<String>,
    pub openai_base_url: Option<String>,
    pub openai_model: Option<String>,
    pub rerank: Option<String>,
    pub rerank_model: Option<String>,
    pub rerank_top_k: Option<usize>,
    pub rerank_initial_k: Option<usize>,
    pub chunk_size: Option<usize>,
    pub chunk_overlap: Option<usize>,
    pub max_per_file: Option<usize>,
    pub summary_model: Option<String>,
    pub rrf_k: Option<u32>,
    pub data_dir: Option<String>,
    pub server_url: Option<String>,
    pub daemon_timeout: Option<u64>,
    pub debug: Option<bool>,
    pub log_file: Option<String>,
}

/// Mapping from FileConfig field names to environment variable names (matching clap env bindings).
const FIELD_ENV_MAP: &[(&str, &str)] = &[
    ("docs_version", "CANGJIE_DOCS_VERSION"),
    ("lang", "CANGJIE_DOCS_LANG"),
    ("embedding", "CANGJIE_EMBEDDING_TYPE"),
    ("local_model", "CANGJIE_LOCAL_MODEL"),
    ("openai_api_key", "OPENAI_API_KEY"),
    ("openai_base_url", "OPENAI_BASE_URL"),
    ("openai_model", "OPENAI_EMBEDDING_MODEL"),
    ("rerank", "CANGJIE_RERANK_TYPE"),
    ("rerank_model", "CANGJIE_RERANK_MODEL"),
    ("rerank_top_k", "CANGJIE_RERANK_TOP_K"),
    ("rerank_initial_k", "CANGJIE_RERANK_INITIAL_K"),
    ("chunk_size", "CANGJIE_CHUNK_MAX_SIZE"),
    ("chunk_overlap", "CANGJIE_CHUNK_OVERLAP"),
    ("max_per_file", "CANGJIE_MAX_PER_FILE"),
    ("summary_model", "CANGJIE_SUMMARY_MODEL"),
    ("rrf_k", "CANGJIE_RRF_K"),
    ("data_dir", "CANGJIE_DATA_DIR"),
    ("server_url", "CANGJIE_SERVER_URL"),
    ("daemon_timeout", "CANGJIE_DAEMON_TIMEOUT"),
    ("debug", "CANGJIE_DEBUG"),
    ("log_file", "CANGJIE_LOG_FILE"),
];

/// Load config file and set environment variables for any keys not already present.
/// This must be called BEFORE `CangjieArgs::parse()` so clap picks up the values.
///
/// Priority: CLI args > env vars > config file > defaults
pub fn load_config_to_env() {
    let path = config_file();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return, // No config file — use defaults
    };

    let config: FileConfig = match toml::from_str(&content) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Warning: failed to parse {}: {e}", path.display());
            return;
        }
    };

    // Convert config to a TOML table for generic field access
    let table = match toml::Value::try_from(&config) {
        Ok(toml::Value::Table(t)) => t,
        _ => return,
    };

    for &(field, env_var) in FIELD_ENV_MAP {
        // Only set if env var is not already present (env > config file)
        if std::env::var(env_var).is_ok() {
            continue;
        }
        if let Some(value) = table.get(field) {
            let s = match value {
                toml::Value::String(s) => s.clone(),
                toml::Value::Integer(n) => n.to_string(),
                toml::Value::Boolean(b) => b.to_string(),
                toml::Value::Float(f) => f.to_string(),
                _ => continue,
            };
            std::env::set_var(env_var, &s);
        }
    }
}

/// Construct Settings from environment variables (set by load_config_to_env).
/// Used by the daemon process which doesn't receive server options via CLI args.
pub fn settings_from_env() -> Settings {
    fn env_str(key: &str, default: &str) -> String {
        std::env::var(key).unwrap_or_else(|_| default.to_string())
    }

    fn env_opt(key: &str) -> Option<String> {
        std::env::var(key).ok().filter(|s| !s.is_empty())
    }

    fn env_usize(key: &str, default: usize) -> usize {
        std::env::var(key)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }

    fn env_u32(key: &str, default: u32) -> u32 {
        std::env::var(key)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }

    fn env_u64(key: &str, default: u64) -> u64 {
        std::env::var(key)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }

    fn env_bool(key: &str, default: bool) -> bool {
        std::env::var(key)
            .ok()
            .map(|v| matches!(v.as_str(), "true" | "1" | "yes"))
            .unwrap_or(default)
    }

    let embedding_type = match env_str("CANGJIE_EMBEDDING_TYPE", "none").as_str() {
        "local" => EmbeddingType::Local,
        "openai" => EmbeddingType::OpenAI,
        _ => EmbeddingType::None,
    };

    let rerank_type = match env_str("CANGJIE_RERANK_TYPE", "none").as_str() {
        "local" => RerankType::Local,
        "openai" => RerankType::OpenAI,
        _ => RerankType::None,
    };

    let docs_lang = match env_str("CANGJIE_DOCS_LANG", "zh").as_str() {
        "en" => DocLang::En,
        _ => DocLang::Zh,
    };

    Settings {
        docs_version: env_str("CANGJIE_DOCS_VERSION", DEFAULT_DOCS_VERSION),
        docs_lang,
        embedding_type,
        local_model: env_str("CANGJIE_LOCAL_MODEL", DEFAULT_LOCAL_MODEL),
        rerank_type,
        rerank_model: env_str("CANGJIE_RERANK_MODEL", DEFAULT_RERANK_MODEL),
        rerank_top_k: env_usize("CANGJIE_RERANK_TOP_K", DEFAULT_RERANK_TOP_K),
        rerank_initial_k: env_usize("CANGJIE_RERANK_INITIAL_K", DEFAULT_RERANK_INITIAL_K),
        rrf_k: env_u32("CANGJIE_RRF_K", DEFAULT_RRF_K),
        max_chunk_chars: std::env::var("CANGJIE_CHUNK_MAX_SIZE")
            .ok()
            .and_then(|v| v.parse().ok()),
        chunk_overlap_chars: env_usize("CANGJIE_CHUNK_OVERLAP", DEFAULT_CHUNK_OVERLAP_CHARS),
        max_per_file: env_usize("CANGJIE_MAX_PER_FILE", DEFAULT_MAX_PER_FILE),
        summary_model: env_opt("CANGJIE_SUMMARY_MODEL"),
        data_dir: env_opt("CANGJIE_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(get_default_data_dir),
        server_url: env_opt("CANGJIE_SERVER_URL"),
        openai_api_key: env_opt("OPENAI_API_KEY"),
        openai_base_url: env_str("OPENAI_BASE_URL", DEFAULT_OPENAI_BASE_URL),
        openai_model: env_str("OPENAI_EMBEDDING_MODEL", DEFAULT_OPENAI_MODEL),
        http_pool_idle_timeout_secs: env_u64(
            "CANGJIE_HTTP_POOL_IDLE_TIMEOUT_SECS",
            DEFAULT_HTTP_POOL_IDLE_TIMEOUT_SECS,
        ),
        http_pool_max_idle_per_host: env_usize(
            "CANGJIE_HTTP_POOL_MAX_IDLE_PER_HOST",
            DEFAULT_HTTP_POOL_MAX_IDLE_PER_HOST,
        ),
        http_tcp_keepalive_secs: env_u64(
            "CANGJIE_HTTP_TCP_KEEPALIVE_SECS",
            DEFAULT_HTTP_TCP_KEEPALIVE_SECS,
        ),
        http_enable_http2: env_bool("CANGJIE_HTTP2", DEFAULT_HTTP_ENABLE_HTTP2),
        ..Settings::default()
    }
}

/// Generate a default config file content with all fields commented out.
pub fn generate_default_config() -> String {
    r#"# Cangjie MCP CLI configuration
# Place this file at the path shown by `cangjie-mcp config path`
#
# Priority: CLI flags > environment variables > this file > built-in defaults

# Documentation version (git tag)
# docs_version = "dev"

# Documentation language: "zh" or "en"
# lang = "zh"

# Embedding type: "none" (BM25 only), "local", or "openai"
# embedding = "none"

# Local HuggingFace embedding model
# local_model = "paraphrase-multilingual-MiniLM-L12-v2"

# OpenAI-compatible API settings
# openai_api_key = "sk-..."
# openai_base_url = "https://api.siliconflow.cn/v1"
# openai_model = "BAAI/bge-m3"

# Rerank settings: "none", "local", or "openai"
# rerank = "none"
# rerank_model = "BAAI/bge-reranker-v2-m3"
# rerank_top_k = 5
# rerank_initial_k = 20

# Chunk settings (omit chunk_size to enable dynamic detection: 800/1200/1600 based on code density)
# chunk_size = 1200
# chunk_overlap = 100
# max_per_file = 2

# LLM model for chunk context summaries
# summary_model = "gpt-4o-mini"

# Reciprocal Rank Fusion constant
# rrf_k = 60

# Data directory (default: ~/.cangjie-mcp)
# data_dir = "/path/to/data"

# Remote server URL (skip local indexing, forward queries)
# server_url = "http://localhost:8765"

# Daemon idle timeout in minutes
# daemon_timeout = 30

# Debug logging
# debug = false

# Log file path
# log_file = "/path/to/cangjie.log"
"#
    .to_string()
}
