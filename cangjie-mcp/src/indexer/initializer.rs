use anyhow::{bail, Context, Result};
use tracing::info;

use crate::config::{IndexInfo, PrebuiltMode, Settings, DEFAULT_EMBEDDING_DIM, VECTOR_BATCH_SIZE};
use crate::indexer::document::chunker::chunk_documents;
use crate::indexer::document::source::{DocumentSource, GitDocumentSource};
use crate::indexer::embedding;
use crate::indexer::search::bm25::BM25Store;
use crate::indexer::search::vector::VectorStore;
use crate::indexer::IndexMetadata;

/// Check if a valid index exists by reading the metadata file.
async fn index_is_ready(index_info: &IndexInfo) -> bool {
    let metadata_path = index_info.index_dir().join("index_metadata.json");
    match tokio::fs::read_to_string(&metadata_path).await {
        Ok(content) => match serde_json::from_str::<IndexMetadata>(&content) {
            Ok(meta) => {
                meta.version == index_info.version
                    && meta.lang == index_info.lang.to_string()
                    && meta.document_count > 0
            }
            Err(_) => false,
        },
        Err(_) => false,
    }
}

/// Build the BM25 (and optionally vector) index from documentation.
async fn build_index(settings: &Settings, index_info: &IndexInfo) -> Result<()> {
    info!("Loading documents...");
    let doc_source = GitDocumentSource::new(index_info.docs_repo_dir(), index_info.lang)?;

    let documents = doc_source.load_all_documents().await?;
    if documents.is_empty() {
        bail!(
            "No documents found for version={}, lang={}",
            index_info.version,
            index_info.lang
        );
    }
    info!("Loaded {} documents", documents.len());

    info!("Chunking documents...");
    let chunks = chunk_documents(documents, settings.chunk_max_size).await;
    info!("Created {} chunks", chunks.len());

    info!("Building BM25 index...");
    let mut bm25 = BM25Store::new(index_info.bm25_index_dir());
    bm25.build_from_chunks(&chunks).await?;

    let embedder = embedding::create_embedder(settings).await.unwrap_or(None);
    if let Some(ref emb) = embedder {
        info!(
            "Building vector index with embedder: {}...",
            emb.model_name()
        );
        let dim = {
            let test = emb.embed(&["test"]).await?;
            test.first()
                .map(|v| v.len())
                .unwrap_or(DEFAULT_EMBEDDING_DIM)
        };
        let mut vs = VectorStore::open(&index_info.vector_db_dir(), dim).await?;
        vs.build_from_chunks(&chunks, emb.as_ref(), VECTOR_BATCH_SIZE)
            .await?;
    }

    let search_mode = if embedder.is_some() { "hybrid" } else { "bm25" };
    let metadata = IndexMetadata {
        version: index_info.version.clone(),
        lang: index_info.lang.to_string(),
        embedding_model: settings.embedding_model_name(),
        document_count: chunks.len(),
        search_mode: search_mode.to_string(),
    };
    let metadata_path = index_info.index_dir().join("index_metadata.json");
    tokio::fs::create_dir_all(metadata_path.parent().context("Invalid metadata path")?).await?;
    let json = serde_json::to_string_pretty(&metadata)?;
    tokio::fs::write(&metadata_path, json).await?;

    info!("Index built successfully!");
    Ok(())
}

/// Discover all version directories under `data_dir/indexes/` that contain a
/// valid index matching the current settings (lang + embedding model).
async fn discover_prebuilt_versions(settings: &Settings) -> Result<Vec<String>> {
    let indexes_dir = settings.data_dir.join("indexes");
    if !indexes_dir.exists() {
        return Ok(Vec::new());
    }

    let mut versions = Vec::new();
    let mut entries = tokio::fs::read_dir(&indexes_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        if !entry.file_type().await?.is_dir() {
            continue;
        }
        let version = entry.file_name().to_string_lossy().to_string();
        let index_info = IndexInfo::from_settings(settings, &version);
        if index_is_ready(&index_info).await {
            versions.push(version);
        }
    }
    versions.sort();
    Ok(versions)
}

/// Load a pre-built index without any git operations.
///
/// `PrebuiltMode::Auto`: scan and auto-select (exactly one must exist).
/// `PrebuiltMode::Version(v)`: use that specific version.
async fn load_prebuilt_index(settings: &Settings) -> Result<IndexInfo> {
    match &settings.prebuilt {
        PrebuiltMode::Version(version) => {
            let index_info = IndexInfo::from_settings(settings, version);
            if !index_is_ready(&index_info).await {
                bail!(
                    "Pre-built index not found for version={}, lang={}, model={}",
                    version,
                    settings.docs_lang,
                    settings.embedding_model_name()
                );
            }
            info!("Using pre-built index (version: {})", version);
            Ok(index_info)
        }
        PrebuiltMode::Auto => {
            let versions = discover_prebuilt_versions(settings).await?;
            match versions.len() {
                0 => bail!(
                    "No pre-built indexes found in {} (lang={}, model={})",
                    settings.data_dir.join("indexes").display(),
                    settings.docs_lang,
                    settings.embedding_model_name()
                ),
                1 => {
                    let version = &versions[0];
                    let index_info = IndexInfo::from_settings(settings, version);
                    info!("Using pre-built index (version: {})", version);
                    Ok(index_info)
                }
                _ => bail!(
                    "Found {} pre-built indexes: [{}]. Use --prebuilt <VERSION> to specify which one.",
                    versions.len(),
                    versions.join(", ")
                ),
            }
        }
        PrebuiltMode::Off => unreachable!(),
    }
}

/// Initialize repository and build index if needed.
pub async fn initialize_and_index(settings: &Settings) -> Result<IndexInfo> {
    if settings.prebuilt.is_prebuilt() {
        return load_prebuilt_index(settings).await;
    }

    use crate::repo::GitManager;

    // Resolve version (ensures repo is cloned, fetched, and checked out)
    let mut git_mgr = GitManager::new(settings.docs_repo_dir());
    let resolved_version = git_mgr
        .resolve_version(&settings.docs_version)
        .await
        .context("Failed to resolve documentation version")?;
    info!(
        "Resolved version: {} -> {}",
        settings.docs_version, resolved_version
    );

    let index_info = IndexInfo::from_settings(settings, &resolved_version);

    if index_is_ready(&index_info).await {
        info!(
            "Index already exists (version: {}, lang: {})",
            resolved_version, settings.docs_lang
        );
        return Ok(index_info);
    }

    // Build index
    build_index(settings, &index_info).await?;

    Ok(index_info)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{DocLang, EmbeddingType, RerankType};
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn test_settings(data_dir: PathBuf) -> Settings {
        Settings {
            docs_version: "dev".to_string(),
            docs_lang: DocLang::Zh,
            embedding_type: EmbeddingType::None,
            local_model: String::new(),
            rerank_type: RerankType::None,
            rerank_model: String::new(),
            rerank_top_k: 5,
            rerank_initial_k: 20,
            rrf_k: 60,
            chunk_max_size: 6000,
            data_dir,
            server_url: None,
            openai_api_key: None,
            openai_base_url: "https://api.example.com".to_string(),
            openai_model: "test".to_string(),
            prebuilt: PrebuiltMode::Off,
        }
    }

    /// Helper: write a valid index_metadata.json at the correct path for the given
    /// settings and version.
    async fn write_valid_metadata(
        data_dir: &std::path::Path,
        version: &str,
        lang: &str,
        doc_count: usize,
    ) {
        let settings = test_settings(data_dir.to_path_buf());
        let index_info = IndexInfo::from_settings(&settings, version);
        let index_dir = index_info.index_dir();
        tokio::fs::create_dir_all(&index_dir).await.unwrap();

        let metadata = IndexMetadata {
            version: version.to_string(),
            lang: lang.to_string(),
            embedding_model: "none".to_string(),
            document_count: doc_count,
            search_mode: "bm25".to_string(),
        };
        let json = serde_json::to_string_pretty(&metadata).unwrap();
        let metadata_path = index_dir.join("index_metadata.json");
        tokio::fs::write(&metadata_path, json).await.unwrap();
    }

    #[tokio::test]
    async fn test_index_is_ready_no_metadata_file() {
        let tmp = TempDir::new().unwrap();
        let settings = test_settings(tmp.path().to_path_buf());
        let index_info = IndexInfo::from_settings(&settings, "v0.55.4");

        assert!(!index_is_ready(&index_info).await);
    }

    #[tokio::test]
    async fn test_index_is_ready_invalid_json() {
        let tmp = TempDir::new().unwrap();
        let settings = test_settings(tmp.path().to_path_buf());
        let index_info = IndexInfo::from_settings(&settings, "v0.55.4");
        let index_dir = index_info.index_dir();
        tokio::fs::create_dir_all(&index_dir).await.unwrap();
        tokio::fs::write(index_dir.join("index_metadata.json"), "not valid json")
            .await
            .unwrap();

        assert!(!index_is_ready(&index_info).await);
    }

    #[tokio::test]
    async fn test_index_is_ready_wrong_version() {
        let tmp = TempDir::new().unwrap();
        // Write metadata for version "v0.55.3"
        write_valid_metadata(tmp.path(), "v0.55.3", "zh", 100).await;

        // But ask about version "v0.55.4"
        let settings = test_settings(tmp.path().to_path_buf());
        let index_info = IndexInfo::from_settings(&settings, "v0.55.4");

        assert!(!index_is_ready(&index_info).await);
    }

    #[tokio::test]
    async fn test_index_is_ready_matching_version() {
        let tmp = TempDir::new().unwrap();
        write_valid_metadata(tmp.path(), "v0.55.4", "zh", 100).await;

        let settings = test_settings(tmp.path().to_path_buf());
        let index_info = IndexInfo::from_settings(&settings, "v0.55.4");

        assert!(index_is_ready(&index_info).await);
    }

    #[tokio::test]
    async fn test_index_is_ready_zero_document_count() {
        let tmp = TempDir::new().unwrap();
        write_valid_metadata(tmp.path(), "v0.55.4", "zh", 0).await;

        let settings = test_settings(tmp.path().to_path_buf());
        let index_info = IndexInfo::from_settings(&settings, "v0.55.4");

        assert!(
            !index_is_ready(&index_info).await,
            "document_count == 0 means index is not ready"
        );
    }

    #[tokio::test]
    async fn test_index_is_ready_wrong_lang() {
        let tmp = TempDir::new().unwrap();
        // Settings use DocLang::Zh ("zh"), so write metadata with lang "en"
        let settings = test_settings(tmp.path().to_path_buf());
        let index_info = IndexInfo::from_settings(&settings, "v0.55.4");
        let index_dir = index_info.index_dir();
        tokio::fs::create_dir_all(&index_dir).await.unwrap();

        let metadata = IndexMetadata {
            version: "v0.55.4".to_string(),
            lang: "en".to_string(), // wrong lang
            embedding_model: "none".to_string(),
            document_count: 100,
            search_mode: "bm25".to_string(),
        };
        let json = serde_json::to_string_pretty(&metadata).unwrap();
        tokio::fs::write(index_dir.join("index_metadata.json"), json)
            .await
            .unwrap();

        assert!(
            !index_is_ready(&index_info).await,
            "Mismatched lang should make index not ready"
        );
    }

    #[tokio::test]
    async fn test_discover_prebuilt_no_indexes_dir() {
        let tmp = TempDir::new().unwrap();
        let settings = test_settings(tmp.path().to_path_buf());

        let versions = discover_prebuilt_versions(&settings).await.unwrap();
        assert!(versions.is_empty());
    }

    #[tokio::test]
    async fn test_discover_prebuilt_empty_indexes_dir() {
        let tmp = TempDir::new().unwrap();
        let indexes_dir = tmp.path().join("indexes");
        tokio::fs::create_dir_all(&indexes_dir).await.unwrap();

        let settings = test_settings(tmp.path().to_path_buf());
        let versions = discover_prebuilt_versions(&settings).await.unwrap();
        assert!(versions.is_empty());
    }

    #[tokio::test]
    async fn test_discover_prebuilt_one_valid_index() {
        let tmp = TempDir::new().unwrap();
        write_valid_metadata(tmp.path(), "v0.55.4", "zh", 100).await;

        let settings = test_settings(tmp.path().to_path_buf());
        let versions = discover_prebuilt_versions(&settings).await.unwrap();
        assert_eq!(versions, vec!["v0.55.4"]);
    }

    #[tokio::test]
    async fn test_discover_prebuilt_multiple_valid_indexes() {
        let tmp = TempDir::new().unwrap();
        write_valid_metadata(tmp.path(), "v0.55.3", "zh", 80).await;
        write_valid_metadata(tmp.path(), "v0.55.4", "zh", 100).await;

        let settings = test_settings(tmp.path().to_path_buf());
        let versions = discover_prebuilt_versions(&settings).await.unwrap();
        assert_eq!(versions, vec!["v0.55.3", "v0.55.4"]);
    }

    #[tokio::test]
    async fn test_discover_prebuilt_skips_invalid_index() {
        let tmp = TempDir::new().unwrap();
        write_valid_metadata(tmp.path(), "v0.55.4", "zh", 100).await;

        // Create an invalid index directory (no valid metadata)
        let invalid_dir = tmp
            .path()
            .join("indexes")
            .join("v0.55.2")
            .join("zh")
            .join("bm25-only");
        tokio::fs::create_dir_all(&invalid_dir).await.unwrap();
        tokio::fs::write(invalid_dir.join("index_metadata.json"), "bad json")
            .await
            .unwrap();

        let settings = test_settings(tmp.path().to_path_buf());
        let versions = discover_prebuilt_versions(&settings).await.unwrap();
        assert_eq!(versions, vec!["v0.55.4"]);
    }

    #[tokio::test]
    async fn test_discover_prebuilt_skips_files_in_indexes_dir() {
        let tmp = TempDir::new().unwrap();
        write_valid_metadata(tmp.path(), "v0.55.4", "zh", 100).await;

        // Create a plain file (not a directory) inside indexes/
        let indexes_dir = tmp.path().join("indexes");
        tokio::fs::write(indexes_dir.join("README.md"), "not a version dir")
            .await
            .unwrap();

        let settings = test_settings(tmp.path().to_path_buf());
        let versions = discover_prebuilt_versions(&settings).await.unwrap();
        assert_eq!(versions, vec!["v0.55.4"]);
    }

    #[tokio::test]
    async fn test_load_prebuilt_version_valid() {
        let tmp = TempDir::new().unwrap();
        write_valid_metadata(tmp.path(), "v0.55.4", "zh", 100).await;

        let mut settings = test_settings(tmp.path().to_path_buf());
        settings.prebuilt = PrebuiltMode::Version("v0.55.4".to_string());

        let index_info = load_prebuilt_index(&settings).await.unwrap();
        assert_eq!(index_info.version, "v0.55.4");
    }

    #[tokio::test]
    async fn test_load_prebuilt_version_missing() {
        let tmp = TempDir::new().unwrap();
        // No metadata written

        let mut settings = test_settings(tmp.path().to_path_buf());
        settings.prebuilt = PrebuiltMode::Version("v0.55.4".to_string());

        let result = load_prebuilt_index(&settings).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Pre-built index not found"),
            "Error should mention 'Pre-built index not found', got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_load_prebuilt_auto_one_index() {
        let tmp = TempDir::new().unwrap();
        write_valid_metadata(tmp.path(), "v0.55.4", "zh", 100).await;

        let mut settings = test_settings(tmp.path().to_path_buf());
        settings.prebuilt = PrebuiltMode::Auto;

        let index_info = load_prebuilt_index(&settings).await.unwrap();
        assert_eq!(index_info.version, "v0.55.4");
    }

    #[tokio::test]
    async fn test_load_prebuilt_auto_zero_indexes() {
        let tmp = TempDir::new().unwrap();
        // Create the indexes dir but leave it empty
        tokio::fs::create_dir_all(tmp.path().join("indexes"))
            .await
            .unwrap();

        let mut settings = test_settings(tmp.path().to_path_buf());
        settings.prebuilt = PrebuiltMode::Auto;

        let result = load_prebuilt_index(&settings).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("No pre-built indexes found"),
            "Error should mention no indexes found, got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_load_prebuilt_auto_multiple_indexes() {
        let tmp = TempDir::new().unwrap();
        write_valid_metadata(tmp.path(), "v0.55.3", "zh", 80).await;
        write_valid_metadata(tmp.path(), "v0.55.4", "zh", 100).await;

        let mut settings = test_settings(tmp.path().to_path_buf());
        settings.prebuilt = PrebuiltMode::Auto;

        let result = load_prebuilt_index(&settings).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Found 2 pre-built indexes"),
            "Error should mention multiple indexes, got: {err_msg}"
        );
    }
}
