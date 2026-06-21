use std::path::PathBuf;

pub const DEFAULT_DOCS_VERSION: &str = "dev";
pub const DOCS_REPO_URL: &str = "https://gitcode.com/Cangjie/cangjie_docs.git";
pub const RUNTIME_REPO_URL: &str = "https://gitcode.com/Cangjie/cangjie_runtime.git";
pub const STDX_REPO_URL: &str = "https://gitcode.com/Cangjie/cangjie_stdx.git";
pub const DEFAULT_LOCAL_MODEL: &str = "paraphrase-multilingual-MiniLM-L12-v2";
pub const DEFAULT_RRF_K: u32 = 60;
pub const DEFAULT_RERANK_MODEL: &str = "BAAI/bge-reranker-v2-m3";
pub const DEFAULT_RERANK_TOP_K: usize = 5;
pub const DEFAULT_RERANK_INITIAL_K: usize = 20;
pub const DEFAULT_CHUNK_OVERLAP_CHARS: usize = 100;
pub const CODE_DENSE_THRESHOLD: f64 = 0.6;
pub const CODE_MIXED_THRESHOLD: f64 = 0.2;
pub const DEFAULT_CODE_DENSE_CHARS: usize = 800;
pub const DEFAULT_CODE_MIXED_CHARS: usize = 1200;
pub const DEFAULT_TEXT_HEAVY_CHARS: usize = 1600;
pub const DEFAULT_OPENAI_BASE_URL: &str = "https://api.siliconflow.cn/v1";
pub const DEFAULT_OPENAI_MODEL: &str = "BAAI/bge-m3";
pub const DEFAULT_DATA_DIR_NAME: &str = ".cangjie-mcp";
pub const DEFAULT_SERVER_HOST: &str = "127.0.0.1";
pub const DEFAULT_SERVER_PORT: u16 = 8765;
pub const DEFAULT_HTTP_POOL_IDLE_TIMEOUT_SECS: u64 = 90;
pub const DEFAULT_HTTP_POOL_MAX_IDLE_PER_HOST: usize = 16;
pub const DEFAULT_HTTP_TCP_KEEPALIVE_SECS: u64 = 60;
pub const DEFAULT_HTTP_CONNECT_TIMEOUT_SECS: u64 = 10;
pub const DEFAULT_HTTP_ENABLE_HTTP2: bool = true;
pub const DEFAULT_SERVER_ENABLE_HTTP2: bool = true;
pub const DEFAULT_MAX_PER_FILE: usize = 2;
pub const DEFAULT_MIN_VECTOR_SCORE: f64 = 0.3;

pub const MIN_TOP_K: usize = 1;
pub const MAX_TOP_K: usize = 20;
pub const DEFAULT_TOP_K: usize = 5;
pub const DEFAULT_EMBEDDING_DIM: usize = 384;
pub const SIMILARITY_THRESHOLD: f64 = 0.6;
pub const MAX_SUGGESTIONS: usize = 5;
pub const PACKAGE_FETCH_MULTIPLIER: usize = 3;
pub const DEFAULT_TOPIC_MAX_LENGTH: usize = 10000;
pub const CATEGORY_FILTER_MULTIPLIER: usize = 4;
pub const VECTOR_BATCH_SIZE: usize = 64;
pub const INDEX_WRITER_HEAP_BYTES: usize = 50_000_000;

pub fn get_default_data_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(DEFAULT_DATA_DIR_NAME)
}
