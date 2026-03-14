use anyhow::{Context, Result};
use tracing::info;

use crate::api_client::ApiClient;
use cangjie_core::api_types::ChatResponse;

/// Chunk context summarizer that calls an OpenAI-compatible Chat API.
#[derive(Clone)]
pub struct ChunkSummarizer {
    api: ApiClient,
}

impl ChunkSummarizer {
    pub fn new(api: ApiClient) -> Self {
        Self { api }
    }

    /// Generate a short context summary for a chunk within its parent document.
    pub async fn summarize(&self, doc_text: &str, chunk_text: &str) -> Result<String> {
        // Truncate document text to avoid exceeding token limits
        let max_doc_len = 8000;
        let doc_preview = if doc_text.len() > max_doc_len {
            &doc_text[..doc_text.floor_char_boundary(max_doc_len)]
        } else {
            doc_text
        };

        let prompt = format!(
            "<document>\n{doc_preview}\n</document>\n\n\
             给出以下 chunk 在文档中的简短上下文描述（1-2句话），\
             说明这段内容讲的是什么：\n\n\
             <chunk>\n{chunk_text}\n</chunk>\n\n\
             只输出描述，不要有其他内容。"
        );

        let resp = self
            .api
            .post("chat/completions")
            .json(&serde_json::json!({
                "model": self.api.model(),
                "messages": [{"role": "user", "content": prompt}],
                "max_tokens": 128,
                "temperature": 0.0,
            }))
            .send()
            .await
            .context("Summary API request failed")?;

        let body: ChatResponse = resp
            .json()
            .await
            .context("Failed to parse summary response")?;

        let summary = body
            .choices
            .first()
            .map(|c| c.message.content.trim().to_string())
            .unwrap_or_default();

        Ok(summary)
    }
}

/// Load cached context summaries from disk.
pub async fn load_context_cache(
    cache_path: &std::path::Path,
) -> std::collections::HashMap<String, String> {
    match tokio::fs::read_to_string(cache_path).await {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => std::collections::HashMap::new(),
    }
}

/// Save context summaries cache to disk.
pub async fn save_context_cache(
    cache_path: &std::path::Path,
    cache: &std::collections::HashMap<String, String>,
) -> Result<()> {
    let json = serde_json::to_string_pretty(cache)?;
    if let Some(parent) = cache_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(cache_path, json).await?;
    Ok(())
}

/// Apply context summaries to chunks by prepending `<context>...</context>\n\n`.
///
/// Uses concurrency-limited LLM calls for chunks that don't have a cached summary.
pub async fn apply_context_summaries(
    chunks: &mut [crate::TextChunk],
    doc_texts: &std::collections::HashMap<String, String>,
    summarizer: &ChunkSummarizer,
    cache_path: &std::path::Path,
) -> Result<()> {
    let mut cache = load_context_cache(cache_path).await;
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(8));
    let mut join_set = tokio::task::JoinSet::new();

    for chunk in chunks.iter() {
        let chunk_id = chunk.metadata.chunk_id.clone();
        if cache.contains_key(&chunk_id) {
            continue;
        }

        let doc_text = doc_texts
            .get(&chunk.metadata.file_path)
            .cloned()
            .unwrap_or_default();
        let chunk_text = chunk.text.clone();
        let sem = semaphore.clone();
        let summarizer = summarizer.clone();

        join_set.spawn(async move {
            let _permit = sem.acquire_owned().await;
            let result = summarizer.summarize(&doc_text, &chunk_text).await;
            (chunk_id, result)
        });
    }

    while let Some(res) = join_set.join_next().await {
        let (chunk_id, result) = res?;
        match result {
            Ok(summary) if !summary.is_empty() => {
                cache.insert(chunk_id, summary);
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!("Failed to generate summary for chunk: {}", e);
            }
        }
    }

    // Apply cached summaries to chunks
    for chunk in chunks.iter_mut() {
        if let Some(summary) = cache.get(&chunk.metadata.chunk_id) {
            chunk.text = format!("<context>{summary}</context>\n\n{}", chunk.text);
        }
    }

    save_context_cache(cache_path, &cache).await?;
    info!(
        "Applied {} context summaries ({} cached)",
        chunks.len(),
        cache.len()
    );
    Ok(())
}
