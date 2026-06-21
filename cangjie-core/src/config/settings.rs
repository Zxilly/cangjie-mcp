use std::path::PathBuf;

use super::constants::*;
use super::enums::{DocLang, EmbeddingType, PrebuiltMode, RerankType};

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
    pub runtime_version: String,
    pub stdx_version: String,
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
            runtime_version: DEFAULT_DOCS_VERSION.to_string(),
            stdx_version: DEFAULT_DOCS_VERSION.to_string(),
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

    pub fn fastembed_cache_dir(&self) -> PathBuf {
        self.data_dir.join("cache").join("fastembed")
    }

    pub fn docs_repo_dir(&self) -> PathBuf {
        self.data_dir.join("docs_repo")
    }

    pub fn runtime_repo_dir(&self) -> PathBuf {
        self.data_dir.join("runtime_repo")
    }

    pub fn stdx_repo_dir(&self) -> PathBuf {
        self.data_dir.join("stdx_repo")
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
    fn test_default_settings_chunk_config() {
        let s = Settings::default();
        assert_eq!(s.chunk_overlap_chars, DEFAULT_CHUNK_OVERLAP_CHARS);
        assert!(s.max_chunk_chars.is_none());
    }

    #[test]
    fn test_fastembed_cache_dir_under_data_dir() {
        let s = Settings {
            data_dir: PathBuf::from("/data"),
            ..Settings::default()
        };
        assert_eq!(
            s.fastembed_cache_dir(),
            PathBuf::from("/data/cache/fastembed")
        );
    }
}
