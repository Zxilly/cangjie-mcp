use anyhow::{bail, Context, Result};
use tracing::{info, warn};

use crate::document::chunker::chunk_documents;
use crate::document::source::{DocumentSource, GitDocumentSource};
use crate::embedding;
use crate::search::bm25::BM25Store;
use crate::search::vector::VectorStore;
use crate::{DocData, IndexMetadata, SearchMode};
use cangjie_core::config::{IndexInfo, Settings, DEFAULT_EMBEDDING_DIM, VECTOR_BATCH_SIZE};

fn extend_or_warn(documents: &mut Vec<DocData>, label: &str, result: Result<Vec<DocData>>) {
    match result {
        Ok(docs) => {
            info!("Loaded {} {label} documents", docs.len());
            documents.extend(docs);
        }
        Err(e) => warn!("Failed to load {label} documents: {e}"),
    }
}

/// Build the BM25 (and optionally vector) index from documentation.
pub(super) async fn build_index(settings: &Settings, index_info: &IndexInfo) -> Result<()> {
    info!("Loading documents...");
    let docs_source = GitDocumentSource::for_docs(index_info.docs_repo_dir(), index_info.lang)?;
    let tools_source = GitDocumentSource::for_tools(index_info.docs_repo_dir(), index_info.lang)?;
    let release_notes_source = GitDocumentSource::for_release_notes(index_info.docs_repo_dir())?;
    let runtime_source =
        GitDocumentSource::for_runtime(index_info.runtime_repo_dir(), index_info.lang)?;
    let stdx_source = GitDocumentSource::for_stdx(index_info.stdx_repo_dir(), index_info.lang)?;

    // Auxiliary sources are best-effort: docs is required, the rest log and skip on failure.
    let (docs_result, tools_result, release_notes_result, runtime_result, stdx_result) = tokio::join!(
        docs_source.load_all_documents(),
        tools_source.load_all_documents(),
        release_notes_source.load_all_documents(),
        runtime_source.load_all_documents(),
        stdx_source.load_all_documents(),
    );
    let mut documents = docs_result?;
    extend_or_warn(&mut documents, "tools", tools_result);
    extend_or_warn(&mut documents, "release-notes", release_notes_result);
    extend_or_warn(&mut documents, "runtime stdlib", runtime_result);
    extend_or_warn(&mut documents, "stdx", stdx_result);
    if documents.is_empty() {
        bail!(
            "No documents found for version={}, lang={}",
            index_info.version,
            index_info.lang
        );
    }
    info!("Loaded {} documents", documents.len());

    // Capture doc texts before consuming documents (avoids a second load_all_documents call).
    let needs_summaries = settings.summary_model.is_some() && settings.openai_api_key.is_some();
    let doc_texts: std::collections::HashMap<String, String> = if needs_summaries {
        documents
            .iter()
            .map(|d| (d.metadata.file_path.clone(), d.text.clone()))
            .collect()
    } else {
        std::collections::HashMap::new()
    };

    // Create embedder early so we can query its input limit for chunk sizing.
    // Falls back to None (BM25-only) if creation fails.
    let embedder = embedding::create_embedder(settings).await.unwrap_or(None);

    info!(
        "Chunking documents (max_chunk_chars={:?}, overlap={})...",
        settings.max_chunk_chars, settings.chunk_overlap_chars
    );
    let mut chunks = chunk_documents(
        documents,
        settings.max_chunk_chars,
        settings.chunk_overlap_chars,
    )
    .await;
    info!("Created {} chunks", chunks.len());

    // Contextual retrieval: generate LLM summaries if summary_model is configured.
    if let Some(ref summary_model) = settings.summary_model {
        if let Some(ref api_key) = settings.openai_api_key {
            info!("Generating context summaries with model: {}", summary_model);
            let api = crate::api_client::ApiClient::new(
                settings,
                api_key,
                summary_model,
                &settings.openai_base_url,
                std::time::Duration::from_secs(120),
            )?;
            let summarizer = crate::document::summarizer::ChunkSummarizer::new(api);
            let cache_path = index_info.index_dir().join("context_cache.json");

            crate::document::summarizer::apply_context_summaries(
                &mut chunks,
                &doc_texts,
                &summarizer,
                &cache_path,
            )
            .await?;
        } else {
            tracing::warn!("summary_model set but no API key provided, skipping context summaries");
        }
    }

    info!("Building BM25 index...");
    let mut bm25 = BM25Store::new(index_info.bm25_index_dir());
    bm25.build_from_chunks(&chunks).await?;

    if let Some(ref emb) = embedder {
        info!(
            "Building vector index with embedder: {}...",
            emb.model_name()
        );
        let dim = {
            let test = emb
                .embed(&["test"], crate::embedding::EmbedKind::Document)
                .await?;
            test.first()
                .map(|v| v.len())
                .unwrap_or(DEFAULT_EMBEDDING_DIM)
        };
        let mut vs = VectorStore::open(&index_info.vector_db_dir(), dim).await?;
        vs.build_from_chunks(&chunks, emb.as_ref(), VECTOR_BATCH_SIZE)
            .await?;
    }

    let search_mode = if embedder.is_some() {
        SearchMode::Hybrid
    } else {
        SearchMode::Bm25
    };
    let metadata = IndexMetadata {
        version: index_info.version.clone(),
        lang: index_info.lang.to_string(),
        embedding_model: settings.embedding_model_name(),
        document_count: chunks.len(),
        search_mode,
    };
    let metadata_path = index_info.index_dir().join("index_metadata.json");
    tokio::fs::create_dir_all(metadata_path.parent().context("Invalid metadata path")?).await?;
    let json = serde_json::to_string_pretty(&metadata)?;
    tokio::fs::write(&metadata_path, json).await?;

    info!("Index built successfully!");
    Ok(())
}
