use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

// ── Defaults ────────────────────────────────────────────────────────────────

pub const DEFAULT_DOCS_VERSION: &str = "dev";
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

// ── Constants ───────────────────────────────────────────────────────────────

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

// ── Type enums ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EmbeddingType {
    None,
    Local,
    #[serde(rename = "openai")]
    OpenAI,
}

impl EmbeddingType {
    pub fn is_enabled(self) -> bool {
        self != EmbeddingType::None
    }
}

impl fmt::Display for EmbeddingType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EmbeddingType::None => write!(f, "none"),
            EmbeddingType::Local => write!(f, "local"),
            EmbeddingType::OpenAI => write!(f, "openai"),
        }
    }
}

impl FromStr for EmbeddingType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "none" => Ok(Self::None),
            "local" => Ok(Self::Local),
            "openai" => Ok(Self::OpenAI),
            _ => Err(format!("unknown embedding type: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RerankType {
    None,
    Local,
    #[serde(rename = "openai")]
    OpenAI,
}

impl fmt::Display for RerankType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RerankType::None => write!(f, "none"),
            RerankType::Local => write!(f, "local"),
            RerankType::OpenAI => write!(f, "openai"),
        }
    }
}

impl FromStr for RerankType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "none" => Ok(Self::None),
            "local" => Ok(Self::Local),
            "openai" => Ok(Self::OpenAI),
            _ => Err(format!("unknown rerank type: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocLang {
    Zh,
    En,
}

impl DocLang {
    pub fn source_dir_name(self) -> &'static str {
        match self {
            DocLang::Zh => "source_zh_cn",
            DocLang::En => "source_en",
        }
    }
}

impl fmt::Display for DocLang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DocLang::Zh => write!(f, "zh"),
            DocLang::En => write!(f, "en"),
        }
    }
}

impl FromStr for DocLang {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "zh" => Ok(Self::Zh),
            "en" => Ok(Self::En),
            _ => Err(format!("unknown doc lang: {s}")),
        }
    }
}

// ── PrebuiltMode ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrebuiltMode {
    Off,
    Auto,
    Version(String),
}

impl PrebuiltMode {
    pub fn is_prebuilt(&self) -> bool {
        !matches!(self, PrebuiltMode::Off)
    }
}

// ── Settings ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Settings {
    pub docs_version: String,
    pub docs_lang: DocLang,
    pub embedding_type: EmbeddingType,
    pub local_model: String,
    pub rerank_type: RerankType,
    pub rerank_model: String,
    pub rerank_top_k: usize,
    pub rerank_initial_k: usize,
    pub rrf_k: u32,
    pub chunk_overlap_chars: usize,
    pub max_chunk_chars: Option<usize>,
    pub data_dir: PathBuf,
    pub server_url: Option<String>,
    pub openai_api_key: Option<String>,
    pub openai_base_url: String,
    pub openai_model: String,
    pub http_pool_idle_timeout_secs: u64,
    pub http_pool_max_idle_per_host: usize,
    pub http_tcp_keepalive_secs: u64,
    pub http_enable_http2: bool,
    pub server_enable_http2: bool,
    pub max_per_file: usize,
    pub summary_model: Option<String>,
    pub prebuilt: PrebuiltMode,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            docs_version: DEFAULT_DOCS_VERSION.to_string(),
            docs_lang: DocLang::Zh,
            embedding_type: EmbeddingType::None,
            local_model: DEFAULT_LOCAL_MODEL.to_string(),
            rerank_type: RerankType::None,
            rerank_model: DEFAULT_RERANK_MODEL.to_string(),
            rerank_top_k: DEFAULT_RERANK_TOP_K,
            rerank_initial_k: DEFAULT_RERANK_INITIAL_K,
            rrf_k: DEFAULT_RRF_K,
            chunk_overlap_chars: DEFAULT_CHUNK_OVERLAP_CHARS,
            max_chunk_chars: None,
            data_dir: get_default_data_dir(),
            server_url: None,
            openai_api_key: None,
            openai_base_url: DEFAULT_OPENAI_BASE_URL.to_string(),
            openai_model: DEFAULT_OPENAI_MODEL.to_string(),
            http_pool_idle_timeout_secs: DEFAULT_HTTP_POOL_IDLE_TIMEOUT_SECS,
            http_pool_max_idle_per_host: DEFAULT_HTTP_POOL_MAX_IDLE_PER_HOST,
            http_tcp_keepalive_secs: DEFAULT_HTTP_TCP_KEEPALIVE_SECS,
            http_enable_http2: DEFAULT_HTTP_ENABLE_HTTP2,
            server_enable_http2: DEFAULT_SERVER_ENABLE_HTTP2,
            max_per_file: DEFAULT_MAX_PER_FILE,
            summary_model: None,
            prebuilt: PrebuiltMode::Off,
        }
    }
}

impl Settings {
    pub fn has_embedding(&self) -> bool {
        self.embedding_type.is_enabled()
    }

    pub fn embedding_model_name(&self) -> String {
        match self.embedding_type {
            EmbeddingType::None => "none".to_string(),
            EmbeddingType::Local => format!("local:{}", self.local_model),
            EmbeddingType::OpenAI => format!("openai:{}", self.openai_model),
        }
    }

    pub fn docs_repo_dir(&self) -> PathBuf {
        self.data_dir.join("docs_repo")
    }
}

// ── IndexInfo ───────────────────────────────────────────────────────────────

fn sanitize_for_path(name: &str) -> String {
    name.replace([':', '/'], "--")
}

#[derive(Debug, Clone)]
pub struct IndexInfo {
    pub version: String,
    pub lang: DocLang,
    pub embedding_model_name: String,
    pub data_dir: PathBuf,
}

impl IndexInfo {
    pub fn from_settings(settings: &Settings, resolved_version: &str) -> Self {
        Self {
            version: resolved_version.to_string(),
            lang: settings.docs_lang,
            embedding_model_name: settings.embedding_model_name(),
            data_dir: settings.data_dir.clone(),
        }
    }

    pub fn index_dir(&self) -> PathBuf {
        let model_dir = if self.embedding_model_name == "none" {
            "bm25-only".to_string()
        } else {
            sanitize_for_path(&self.embedding_model_name)
        };
        self.data_dir
            .join("indexes")
            .join(&self.version)
            .join(self.lang.to_string())
            .join(model_dir)
    }

    pub fn bm25_index_dir(&self) -> PathBuf {
        self.index_dir().join("bm25_index")
    }

    pub fn vector_db_dir(&self) -> PathBuf {
        self.index_dir().join("vector_db")
    }

    pub fn docs_repo_dir(&self) -> PathBuf {
        self.data_dir.join("docs_repo")
    }

    pub fn docs_source_dir(&self) -> PathBuf {
        self.docs_repo_dir()
            .join("docs")
            .join("dev-guide")
            .join(self.lang.source_dir_name())
    }
}

// ── Startup Info ────────────────────────────────────────────────────────────

pub fn log_startup_info(settings: &Settings, index_info: &IndexInfo) {
    use tracing::info;

    info!("Cangjie MCP v{}", crate::VERSION);

    if let Some(ref url) = settings.server_url {
        info!("Mode: remote -> {url}");
    } else {
        let search_mode = if settings.has_embedding() {
            "hybrid (BM25 + vector)"
        } else {
            "BM25"
        };
        info!("Search: {search_mode}");
        info!(
            "Chunk: overlap_chars={}, max_chunk_chars={:?}",
            settings.chunk_overlap_chars, settings.max_chunk_chars,
        );
        if settings.has_embedding() {
            let model = match settings.embedding_type {
                EmbeddingType::Local => &settings.local_model,
                _ => &settings.openai_model,
            };
            info!("Embedding: {} / {model}", settings.embedding_type);
        }
    }

    match settings.rerank_type {
        RerankType::None => {}
        _ => {
            info!(
                "Rerank: {} / {} (top_k={}, initial_k={})",
                settings.rerank_type,
                settings.rerank_model,
                settings.rerank_top_k,
                settings.rerank_initial_k,
            );
        }
    }

    info!("Version: {}", index_info.version);
    info!("Language: {}", index_info.lang);
    if settings.has_embedding() {
        info!("Model: {}", index_info.embedding_model_name);
    }
    if settings.server_url.is_none() {
        info!("Index dir: {}", index_info.index_dir().display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_settings() -> Settings {
        Settings {
            docs_version: "0.55.3".to_string(),
            data_dir: PathBuf::from("/tmp/test-data"),
            openai_base_url: "https://api.example.com".to_string(),
            openai_model: "test-model".to_string(),
            ..Settings::default()
        }
    }

    #[test]
    fn test_settings_has_embedding() {
        let mut s = test_settings();
        assert!(!s.has_embedding());
        s.embedding_type = EmbeddingType::OpenAI;
        assert!(s.has_embedding());
        s.embedding_type = EmbeddingType::Local;
        assert!(s.has_embedding());
    }

    #[test]
    fn test_settings_embedding_model_name() {
        let mut s = test_settings();
        assert_eq!(s.embedding_model_name(), "none");

        s.embedding_type = EmbeddingType::Local;
        s.local_model = "my-model".to_string();
        assert_eq!(s.embedding_model_name(), "local:my-model");

        s.embedding_type = EmbeddingType::OpenAI;
        s.openai_model = "text-embed-3".to_string();
        assert_eq!(s.embedding_model_name(), "openai:text-embed-3");
    }

    #[test]
    fn test_index_info_paths() {
        let info = IndexInfo {
            version: "0.55.3".to_string(),
            lang: DocLang::Zh,
            embedding_model_name: "none".to_string(),
            data_dir: PathBuf::from("/data"),
        };

        assert_eq!(
            info.index_dir(),
            PathBuf::from("/data/indexes/0.55.3/zh/bm25-only")
        );
        assert_eq!(
            info.bm25_index_dir(),
            PathBuf::from("/data/indexes/0.55.3/zh/bm25-only/bm25_index")
        );
        assert_eq!(
            info.vector_db_dir(),
            PathBuf::from("/data/indexes/0.55.3/zh/bm25-only/vector_db")
        );
    }

    #[test]
    fn test_index_info_embedding_model_path() {
        let info = IndexInfo {
            version: "dev".to_string(),
            lang: DocLang::En,
            embedding_model_name: "openai:BAAI/bge-m3".to_string(),
            data_dir: PathBuf::from("/data"),
        };

        assert_eq!(
            info.index_dir(),
            PathBuf::from("/data/indexes/dev/en/openai--BAAI--bge-m3")
        );
    }

    #[test]
    fn test_sanitize_for_path() {
        assert_eq!(
            sanitize_for_path("openai:model/name"),
            "openai--model--name"
        );
        assert_eq!(sanitize_for_path("simple"), "simple");
    }

    #[test]
    fn test_embedding_type_from_str() {
        assert_eq!(
            "none".parse::<EmbeddingType>().unwrap(),
            EmbeddingType::None
        );
        assert_eq!(
            "local".parse::<EmbeddingType>().unwrap(),
            EmbeddingType::Local
        );
        assert_eq!(
            "openai".parse::<EmbeddingType>().unwrap(),
            EmbeddingType::OpenAI
        );
        assert!("invalid".parse::<EmbeddingType>().is_err());
    }

    #[test]
    fn test_rerank_type_from_str() {
        assert_eq!("none".parse::<RerankType>().unwrap(), RerankType::None);
        assert_eq!("local".parse::<RerankType>().unwrap(), RerankType::Local);
        assert_eq!("openai".parse::<RerankType>().unwrap(), RerankType::OpenAI);
        assert!("invalid".parse::<RerankType>().is_err());
    }

    #[test]
    fn test_default_settings_chunk_config() {
        let s = Settings::default();
        assert_eq!(s.chunk_overlap_chars, DEFAULT_CHUNK_OVERLAP_CHARS);
        assert!(s.max_chunk_chars.is_none());
    }

    #[test]
    fn test_doc_lang_from_str() {
        assert_eq!("zh".parse::<DocLang>().unwrap(), DocLang::Zh);
        assert_eq!("en".parse::<DocLang>().unwrap(), DocLang::En);
        assert!("invalid".parse::<DocLang>().is_err());
    }
}
