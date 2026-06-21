use std::path::PathBuf;

use super::enums::{DocLang, EmbeddingType, RerankType};
use super::settings::Settings;

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

    pub fn runtime_repo_dir(&self) -> PathBuf {
        self.data_dir.join("runtime_repo")
    }

    pub fn stdx_repo_dir(&self) -> PathBuf {
        self.data_dir.join("stdx_repo")
    }

    pub fn docs_source_dir(&self) -> PathBuf {
        self.docs_repo_dir()
            .join("docs")
            .join("dev-guide")
            .join(self.lang.source_dir_name())
    }
}

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

        if matches!(settings.embedding_type, EmbeddingType::Local)
            || matches!(settings.rerank_type, RerankType::Local)
        {
            info!(
                "Fastembed cache: {}",
                settings.fastembed_cache_dir().display()
            );
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
}
