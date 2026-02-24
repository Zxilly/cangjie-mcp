use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use rusqlite::Connection;
use tracing::info;
use zerocopy::IntoBytes;

use super::sqlite_vec_ext::register_sqlite_vec;
use crate::config::CATEGORY_FILTER_MULTIPLIER;
use crate::indexer::embedding::Embedder;
use crate::indexer::{SearchResult, SearchResultMetadata, TextChunk};

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
                .embed(&texts)
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
        let chunks_owned: Vec<(String, String, String, String, String, bool)> = chunks
            .iter()
            .map(|c| {
                (
                    c.text.clone(),
                    c.metadata.file_path.clone(),
                    c.metadata.category.clone(),
                    c.metadata.topic.clone(),
                    c.metadata.title.clone(),
                    c.metadata.has_code,
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
                    has_code  INTEGER NOT NULL
                );
                CREATE INDEX idx_chunks_category ON chunks(category);
                CREATE VIRTUAL TABLE chunks_vec USING vec0(
                    embedding float[{dim}]
                );"
            ))
            .context("Failed to create tables")?;

            conn.execute_batch("BEGIN")?;

            let mut insert_chunk = conn
                .prepare_cached(
                    "INSERT INTO chunks (id, text, file_path, category, topic, title, has_code)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                )
                .context("Failed to prepare chunk insert")?;

            let mut insert_vec = conn
                .prepare_cached("INSERT INTO chunks_vec (rowid, embedding) VALUES (?1, ?2)")
                .context("Failed to prepare vec insert")?;

            for (idx, ((text, file_path, category, topic, title, has_code), emb)) in
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
                    "SELECT text, file_path, category, topic, title, has_code
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
                    ))
                });

                if let Ok((text, file_path, cat, topic, title, has_code)) = row {
                    // Category filter
                    if let Some(ref filter_cat) = category_owned {
                        if cat != *filter_cat {
                            continue;
                        }
                    }

                    let score = 1.0 / (1.0 + *distance as f64);
                    results.push(SearchResult {
                        text,
                        score,
                        metadata: SearchResultMetadata {
                            file_path,
                            category: cat,
                            topic,
                            title,
                            has_code,
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
}
