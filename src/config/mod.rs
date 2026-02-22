use std::fmt;
use std::path::PathBuf;

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

// ── Defaults ────────────────────────────────────────────────────────────────

pub const DEFAULT_DOCS_VERSION: &str = "dev";
pub const DEFAULT_LOCAL_MODEL: &str = "paraphrase-multilingual-MiniLM-L12-v2";
pub const DEFAULT_RRF_K: u32 = 60;
pub const DEFAULT_RERANK_MODEL: &str = "BAAI/bge-reranker-v2-m3";
pub const DEFAULT_RERANK_TOP_K: usize = 5;
pub const DEFAULT_RERANK_INITIAL_K: usize = 20;
pub const DEFAULT_CHUNK_MAX_SIZE: usize = 6000;
pub const DEFAULT_OPENAI_BASE_URL: &str = "https://api.siliconflow.cn/v1";
pub const DEFAULT_OPENAI_MODEL: &str = "BAAI/bge-m3";
pub const DEFAULT_DATA_DIR_NAME: &str = ".cangjie-mcp";
pub const DEFAULT_SERVER_HOST: &str = "127.0.0.1";
pub const DEFAULT_SERVER_PORT: u16 = 8765;

// ── Constants ───────────────────────────────────────────────────────────────

pub const MIN_TOP_K: usize = 1;
pub const MAX_TOP_K: usize = 20;
pub const DEFAULT_TOP_K: usize = 5;
pub const DEFAULT_EMBEDDING_DIM: usize = 384;
pub const SIMILARITY_THRESHOLD: f64 = 0.6;
pub const MAX_SUGGESTIONS: usize = 5;
pub const PACKAGE_FETCH_MULTIPLIER: usize = 3;
pub const CATEGORY_FILTER_MULTIPLIER: usize = 4;
pub const VECTOR_BATCH_SIZE: usize = 64;
pub const INDEX_WRITER_HEAP_BYTES: usize = 50_000_000;

pub fn get_default_data_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(DEFAULT_DATA_DIR_NAME)
}

// ── Type enums ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum EmbeddingType {
    None,
    Local,
    #[serde(rename = "openai")]
    #[value(name = "openai")]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum RerankType {
    None,
    Local,
    #[serde(rename = "openai")]
    #[value(name = "openai")]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
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
    pub chunk_max_size: usize,
    pub data_dir: PathBuf,
    pub server_url: Option<String>,
    pub openai_api_key: Option<String>,
    pub openai_base_url: String,
    pub openai_model: String,
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

pub fn format_startup_info(settings: &Settings, index_info: &IndexInfo) -> String {
    let mut lines = Vec::new();
    lines.push(String::new());
    lines.push(format!("  Cangjie MCP v{}", crate::VERSION));
    lines.push("  ┌─ Configuration ─────────────────────────────".to_string());

    if let Some(ref url) = settings.server_url {
        lines.push(format!("  │ Mode       : remote → {url}"));
    } else {
        let search_mode = if settings.has_embedding() {
            "hybrid (BM25 + vector)"
        } else {
            "BM25"
        };
        lines.push(format!("  │ Search     : {search_mode}"));
        if settings.has_embedding() {
            let model = match settings.embedding_type {
                EmbeddingType::Local => &settings.local_model,
                _ => &settings.openai_model,
            };
            lines.push(format!(
                "  │ Embedding  : {} · {model}",
                settings.embedding_type
            ));
        }
    }

    match settings.rerank_type {
        RerankType::None => {
            lines.push("  │ Rerank     : disabled".to_string());
        }
        _ => {
            lines.push(format!(
                "  │ Rerank     : {} · {}",
                settings.rerank_type, settings.rerank_model
            ));
            lines.push(format!(
                "  │              top_k={}  initial_k={}",
                settings.rerank_top_k, settings.rerank_initial_k
            ));
        }
    }

    lines.push("  ├─ Index ──────────────────────────────────────".to_string());
    lines.push(format!("  │ Version    : {}", index_info.version));
    lines.push(format!("  │ Language   : {}", index_info.lang));
    if settings.has_embedding() {
        lines.push(format!(
            "  │ Model      : {}",
            index_info.embedding_model_name
        ));
    }

    if settings.server_url.is_none() {
        lines.push(format!(
            "  │ Data Dir   : {}",
            index_info.data_dir.display()
        ));
        lines.push(format!(
            "  │ Index Dir  : {}",
            index_info.index_dir().display()
        ));
    }

    lines.push("  └─────────────────────────────────────────────".to_string());
    lines.push(String::new());

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_settings() -> Settings {
        Settings {
            docs_version: "0.55.3".to_string(),
            docs_lang: DocLang::Zh,
            embedding_type: EmbeddingType::None,
            local_model: "".to_string(),
            rerank_type: RerankType::None,
            rerank_model: "".to_string(),
            rerank_top_k: 5,
            rerank_initial_k: 20,
            rrf_k: 60,
            chunk_max_size: 6000,
            data_dir: PathBuf::from("/tmp/test-data"),
            server_url: None,
            openai_api_key: None,
            openai_base_url: "https://api.example.com".to_string(),
            openai_model: "test-model".to_string(),
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
    fn test_settings_docs_repo_dir() {
        let s = test_settings();
        assert_eq!(s.docs_repo_dir(), PathBuf::from("/tmp/test-data/docs_repo"));
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
    fn test_index_info_docs_source_dir() {
        let info = IndexInfo {
            version: "dev".to_string(),
            lang: DocLang::Zh,
            embedding_model_name: "none".to_string(),
            data_dir: PathBuf::from("/data"),
        };
        assert!(info
            .docs_source_dir()
            .to_string_lossy()
            .contains("source_zh_cn"));

        let info_en = IndexInfo {
            version: "dev".to_string(),
            lang: DocLang::En,
            embedding_model_name: "none".to_string(),
            data_dir: PathBuf::from("/data"),
        };
        assert!(info_en
            .docs_source_dir()
            .to_string_lossy()
            .contains("source_en"));
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
    fn test_format_startup_info_local() {
        let s = test_settings();
        let info = IndexInfo::from_settings(&s, "0.55.3");
        let output = format_startup_info(&s, &info);
        assert!(output.contains("BM25"));
        assert!(output.contains("0.55.3"));
    }

    #[test]
    fn test_format_startup_info_remote() {
        let mut s = test_settings();
        s.server_url = Some("http://localhost:8765".to_string());
        let info = IndexInfo::from_settings(&s, "0.55.3");
        let output = format_startup_info(&s, &info);
        assert!(output.contains("remote"));
        assert!(output.contains("http://localhost:8765"));
    }
}
