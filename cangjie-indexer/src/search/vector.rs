use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use rusqlite::Connection;
use tracing::info;
use zerocopy::IntoBytes;

use super::sqlite_vec_ext::register_sqlite_vec;
use crate::embedding::{EmbedKind, Embedder};
use crate::{SearchResult, SearchResultMetadata, TextChunk};
use cangjie_core::config::{CATEGORY_FILTER_MULTIPLIER, DEFAULT_MIN_VECTOR_SCORE};

pub struct VectorStore {
    conn: Arc<std::sync::Mutex<Connection>>,
    ready: bool,
    dim: usize,
}

impl VectorStore {
    pub async fn open(path: &Path, dim: usize) -> Result<Self> {
        let path = path.to_path_buf();
        let d = dim;
        tokio::task::spawn_blocking(move || Self::open_sync(&path, d))
            .await
            .context("spawn_blocking join error")?
    }

    fn open_sync(path: &Path, dim: usize) -> Result<Self> {
        std::fs::create_dir_all(path)
            .with_context(|| format!("Failed to create vector store dir: {path:?}"))?;

        register_sqlite_vec()?;

        let db_path = path.join("vectors.db");
        let conn = Connection::open(&db_path)
            .with_context(|| format!("Failed to open SQLite DB at {db_path:?}"))?;

        // Check whether the data tables already exist and contain rows.
        let ready = conn
            .prepare("SELECT COUNT(*) FROM chunks")
            .and_then(|mut s| s.query_row([], |r| r.get::<_, i64>(0)))
            .unwrap_or(0)
            > 0;

        Ok(Self {
            conn: Arc::new(std::sync::Mutex::new(conn)),
            ready,
            dim,
        })
    }

    pub fn is_ready(&self) -> bool {
        self.ready
    }

    pub async fn build_from_chunks(
        &mut self,
        chunks: &[TextChunk],
        embedder: &dyn Embedder,
        batch_size: usize,
    ) -> Result<()> {
        if chunks.is_empty() {
            anyhow::bail!("No chunks provided for vector index");
        }

        info!(
            "Building vector index from {} chunks (batch_size={})...",
            chunks.len(),
            batch_size
        );

        // Phase 1: embed all chunks (async)
        let mut all_embeddings: Vec<Vec<f32>> = Vec::with_capacity(chunks.len());
        for (i, batch_chunks) in chunks.chunks(batch_size).enumerate() {
            let texts: Vec<&str> = batch_chunks.iter().map(|c| c.text.as_str()).collect();
            let embeddings = embedder
                .embed(&texts, EmbedKind::Document)
                .await
                .context("Embedding batch failed")?;
            all_embeddings.extend(embeddings);
            info!(
                "Embedded batch {}/{} ({} chunks)",
                i + 1,
                chunks.len().div_ceil(batch_size),
                batch_chunks.len()
            );
        }

        if all_embeddings.is_empty() {
            anyhow::bail!("No embeddings generated");
        }

        // Phase 2: insert into SQLite (blocking)
        let conn = Arc::clone(&self.conn);
        let dim = self.dim;
        let chunks_owned: Vec<(String, String, String, String, String, bool, String)> = chunks
            .iter()
            .map(|c| {
                (
                    c.text.clone(),
                    c.metadata.file_path.clone(),
                    c.metadata.category.clone(),
                    c.metadata.topic.clone(),
                    c.metadata.title.clone(),
                    c.metadata.has_code,
                    c.metadata.chunk_id.clone(),
                )
            })
            .collect();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("mutex poisoned");

            // Drop old tables and recreate
            conn.execute_batch("DROP TABLE IF EXISTS chunks_vec; DROP TABLE IF EXISTS chunks;")
                .context("Failed to drop old tables")?;

            conn.execute_batch(&format!(
                "CREATE TABLE chunks (
                    id        INTEGER PRIMARY KEY,
                    text      TEXT NOT NULL,
                    file_path TEXT NOT NULL,
                    category  TEXT NOT NULL,
                    topic     TEXT NOT NULL,
                    title     TEXT NOT NULL,
                    has_code  INTEGER NOT NULL,
                    chunk_id  TEXT NOT NULL DEFAULT ''
                );
                CREATE INDEX idx_chunks_category ON chunks(category);
                CREATE INDEX idx_chunks_chunk_id ON chunks(chunk_id);
                CREATE VIRTUAL TABLE chunks_vec USING vec0(
                    embedding float[{dim}]
                );"
            ))
            .context("Failed to create tables")?;

            conn.execute_batch("BEGIN")?;

            let mut insert_chunk = conn
                .prepare_cached(
                    "INSERT INTO chunks (id, text, file_path, category, topic, title, has_code, chunk_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                )
                .context("Failed to prepare chunk insert")?;

            let mut insert_vec = conn
                .prepare_cached("INSERT INTO chunks_vec (rowid, embedding) VALUES (?1, ?2)")
                .context("Failed to prepare vec insert")?;

            for (idx, ((text, file_path, category, topic, title, has_code, chunk_id), emb)) in
                chunks_owned.iter().zip(all_embeddings.iter()).enumerate()
            {
                let rowid = (idx + 1) as i64;
                insert_chunk.execute(rusqlite::params![
                    rowid,
                    text,
                    file_path,
                    category,
                    topic,
                    title,
                    *has_code as i32,
                    chunk_id,
                ])?;
                insert_vec.execute(rusqlite::params![rowid, emb.as_bytes()])?;
            }

            drop(insert_chunk);
            drop(insert_vec);
            conn.execute_batch("COMMIT")?;

            Ok::<(), anyhow::Error>(())
        })
        .await
        .context("spawn_blocking join error")??;

        self.ready = true;
        info!("Vector index built successfully.");
        Ok(())
    }

    pub async fn search(
        &self,
        query_emb: &[f32],
        top_k: usize,
        category: Option<&str>,
    ) -> Result<Vec<SearchResult>> {
        if !self.ready {
            return Ok(Vec::new());
        }

        let conn = Arc::clone(&self.conn);
        let query_bytes = query_emb.as_bytes().to_vec();
        let fetch_limit = if category.is_some() {
            top_k * CATEGORY_FILTER_MULTIPLIER
        } else {
            top_k
        };
        let category_owned = category.map(|s| s.to_string());

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("mutex poisoned");

            // KNN search via sqlite-vec
            let mut knn_stmt = conn
                .prepare(
                    "SELECT v.rowid, v.distance
                     FROM chunks_vec v
                     WHERE v.embedding MATCH ?1
                     ORDER BY v.distance
                     LIMIT ?2",
                )
                .context("Failed to prepare KNN query")?;

            let matches: Vec<(i64, f32)> = knn_stmt
                .query_map(rusqlite::params![query_bytes, fetch_limit as i64], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })?
                .filter_map(|r| r.ok())
                .collect();

            let mut meta_stmt = conn
                .prepare_cached(
                    "SELECT text, file_path, category, topic, title, has_code, chunk_id
                     FROM chunks WHERE id = ?1",
                )
                .context("Failed to prepare metadata query")?;

            let mut results = Vec::with_capacity(top_k);
            for (rowid, distance) in &matches {
                let row = meta_stmt.query_row([rowid], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, bool>(5)?,
                        r.get::<_, String>(6)?,
                    ))
                });

                if let Ok((text, file_path, cat, topic, title, has_code, chunk_id)) = row {
                    // Category filter
                    if let Some(ref filter_cat) = category_owned {
                        if cat != *filter_cat {
                            continue;
                        }
                    }

                    let score = 1.0 / (1.0 + *distance as f64);
                    if score < DEFAULT_MIN_VECTOR_SCORE {
                        continue;
                    }
                    // Use stored chunk_id, fall back to synthesized one for legacy data
                    let chunk_id = if chunk_id.is_empty() {
                        format!("{}#{}", file_path, rowid - 1)
                    } else {
                        chunk_id
                    };
                    results.push(SearchResult {
                        text,
                        score,
                        metadata: SearchResultMetadata {
                            file_path,
                            category: cat,
                            topic,
                            title,
                            has_code,
                            chunk_id,
                        },
                    });

                    if results.len() >= top_k {
                        break;
                    }
                }
            }

            Ok(results)
        })
        .await
        .context("spawn_blocking join error")?
    }

    /// Fetch chunk text by its chunk_id (e.g. "file_path#idx").
    pub async fn get_chunk_text(&self, chunk_id: &str) -> Option<String> {
        if !self.ready {
            return None;
        }
        let conn = Arc::clone(&self.conn);
        let chunk_id = chunk_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().expect("mutex poisoned");
            let mut stmt = conn
                .prepare_cached("SELECT text FROM chunks WHERE chunk_id = ?1")
                .ok()?;
            stmt.query_row([&chunk_id], |r| r.get::<_, String>(0)).ok()
        })
        .await
        .ok()?
    }
}

/// Parse chunk_id format `"file_path#idx"`.
fn parse_chunk_id(chunk_id: &str) -> Option<(&str, usize)> {
    let hash_pos = chunk_id.rfind('#')?;
    let idx: usize = chunk_id[hash_pos + 1..].parse().ok()?;
    Some((&chunk_id[..hash_pos], idx))
}

/// Expand search results with adjacent chunk context (sentence window).
///
/// For each result, fetches `+-window` neighboring chunks by chunk_id
/// and prepends/appends their text to provide surrounding context.
pub async fn expand_with_window(
    results: Vec<SearchResult>,
    vector_store: &VectorStore,
    window: usize,
) -> Vec<SearchResult> {
    if window == 0 {
        return results;
    }

    let mut expanded = Vec::with_capacity(results.len());
    for mut result in results {
        if let Some((file_path, idx)) = parse_chunk_id(&result.metadata.chunk_id) {
            let mut parts: Vec<String> = Vec::new();

            // Fetch preceding chunks
            for w in (1..=window).rev() {
                if idx >= w {
                    let neighbor_id = format!("{}#{}", file_path, idx - w);
                    if let Some(text) = vector_store.get_chunk_text(&neighbor_id).await {
                        parts.push(text);
                    }
                }
            }

            // Current chunk
            parts.push(result.text.clone());

            // Fetch following chunks
            for w in 1..=window {
                let neighbor_id = format!("{}#{}", file_path, idx + w);
                if let Some(text) = vector_store.get_chunk_text(&neighbor_id).await {
                    parts.push(text);
                }
            }

            if parts.len() > 1 {
                result.text = parts.join("\n\n");
            }
        }
        expanded.push(result);
    }
    expanded
}
