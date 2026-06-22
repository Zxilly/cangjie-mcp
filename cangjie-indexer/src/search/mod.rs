pub mod bm25;
pub mod fusion;
mod local;
mod remote;
mod sqlite_vec_ext;
pub mod synonyms;
pub mod vector;

use std::sync::{Arc, LazyLock};

use jieba_rs::Jieba;

pub use local::LocalSearchIndex;
pub use remote::RemoteSearchIndex;

/// Global Jieba instance shared across all search components.
pub static GLOBAL_JIEBA: LazyLock<Arc<Jieba>> = LazyLock::new(|| Arc::new(Jieba::new()));

/// Shared test fixture used by the `local` and `remote` submodule tests.
#[cfg(test)]
pub(crate) fn test_settings(data_dir: std::path::PathBuf) -> cangjie_core::config::Settings {
    use cangjie_core::config::{DocLang, EmbeddingType, RerankType, Settings};
    Settings {
        data_dir,
        openai_base_url: "https://api.example.com".to_string(),
        openai_model: "test".to_string(),
        docs_lang: DocLang::Zh,
        embedding_type: EmbeddingType::None,
        rerank_type: RerankType::None,
        ..Settings::default()
    }
}
