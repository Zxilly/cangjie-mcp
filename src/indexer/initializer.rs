use anyhow::{bail, Context, Result};
use tracing::info;

use crate::config::{IndexInfo, Settings, DEFAULT_EMBEDDING_DIM, VECTOR_BATCH_SIZE};
use crate::indexer::document::chunker::chunk_documents;
use crate::indexer::document::source::{DocumentSource, GitDocumentSource};
use crate::indexer::embedding;
use crate::indexer::search::bm25::BM25Store;
use crate::indexer::search::vector::VectorStore;
use crate::indexer::IndexMetadata;

/// Check if a valid index exists by reading the metadata file.
fn index_is_ready(index_info: &IndexInfo) -> bool {
    let metadata_path = index_info.index_dir().join("index_metadata.json");
    if !metadata_path.exists() {
        return false;
    }
    match std::fs::read_to_string(&metadata_path) {
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
fn build_index(settings: &Settings, index_info: &IndexInfo) -> Result<()> {
    info!("Loading documents...");
    let doc_source = GitDocumentSource::new(index_info.docs_repo_dir(), index_info.lang)?;

    let documents = doc_source.load_all_documents()?;
    if documents.is_empty() {
        bail!(
            "No documents found for version={}, lang={}",
            index_info.version,
            index_info.lang
        );
    }
    info!("Loaded {} documents", documents.len());

    // Chunk documents
    info!("Chunking documents...");
    let chunks = chunk_documents(&documents, settings.chunk_max_size);
    info!("Created {} chunks", chunks.len());

    // Build BM25 index
    info!("Building BM25 index...");
    let mut bm25 = BM25Store::new(index_info.bm25_index_dir());
    bm25.build_from_chunks(&chunks)?;

    // Build vector index if embedder is available
    let embedder = embedding::create_embedder(settings).unwrap_or(None);
    if let Some(ref emb) = embedder {
        info!(
            "Building vector index with embedder: {}...",
            emb.model_name()
        );
        let dim = {
            let test = emb.embed(&["test"])?;
            test.first()
                .map(|v| v.len())
                .unwrap_or(DEFAULT_EMBEDDING_DIM)
        };
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            let mut vs = VectorStore::open(&index_info.vector_db_dir(), dim).await?;
            vs.build_from_chunks(&chunks, emb.as_ref(), VECTOR_BATCH_SIZE)
                .await
        })?;
    }

    // Write index metadata
    let search_mode = if embedder.is_some() { "hybrid" } else { "bm25" };
    let metadata = IndexMetadata {
        version: index_info.version.clone(),
        lang: index_info.lang.to_string(),
        embedding_model: settings.embedding_model_name(),
        document_count: chunks.len(),
        search_mode: search_mode.to_string(),
    };
    let metadata_path = index_info.index_dir().join("index_metadata.json");
    std::fs::create_dir_all(metadata_path.parent().context("Invalid metadata path")?)?;
    let json = serde_json::to_string_pretty(&metadata)?;
    std::fs::write(&metadata_path, json)?;

    info!("Index built successfully!");
    Ok(())
}

/// Initialize repository and build index if needed.
pub fn initialize_and_index(settings: &Settings) -> Result<IndexInfo> {
    use crate::repo::GitManager;

    // Resolve version (ensures repo is cloned, fetched, and checked out)
    let mut git_mgr = GitManager::new(settings.docs_repo_dir());
    let resolved_version = git_mgr
        .resolve_version(&settings.docs_version)
        .context("Failed to resolve documentation version")?;
    info!(
        "Resolved version: {} -> {}",
        settings.docs_version, resolved_version
    );

    let index_info = IndexInfo::from_settings(settings, &resolved_version);

    if index_is_ready(&index_info) {
        info!(
            "Index already exists (version: {}, lang: {})",
            resolved_version, settings.docs_lang
        );
        return Ok(index_info);
    }

    // Build index
    build_index(settings, &index_info)?;
    Ok(index_info)
}
