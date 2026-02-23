use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use arrow_array::{
    BooleanArray, FixedSizeListArray, RecordBatch, RecordBatchIterator, StringArray,
};
use arrow_schema::{DataType, Field, Schema};
use lancedb::query::{ExecutableQuery, QueryBase};
use tracing::info;

use crate::config::CATEGORY_FILTER_MULTIPLIER;
use crate::indexer::embedding::Embedder;
use crate::indexer::{SearchResult, SearchResultMetadata, TextChunk};

pub struct VectorStore {
    db: lancedb::Connection,
    table: Option<lancedb::Table>,
    dim: usize,
}

impl VectorStore {
    pub async fn open(path: &Path, dim: usize) -> Result<Self> {
        let db = lancedb::connect(path.to_str().context("Invalid UTF-8 in path")?)
            .execute()
            .await
            .context("Failed to open LanceDB")?;

        let table = db.open_table("chunks").execute().await.ok();

        Ok(Self { db, table, dim })
    }

    fn schema(dim: usize) -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    dim as i32,
                ),
                false,
            ),
            Field::new("text", DataType::Utf8, false),
            Field::new("file_path", DataType::Utf8, false),
            Field::new("category", DataType::Utf8, false),
            Field::new("topic", DataType::Utf8, false),
            Field::new("title", DataType::Utf8, false),
            Field::new("has_code", DataType::Boolean, false),
        ]))
    }

    pub async fn build_from_chunks(
        &mut self,
        chunks: &[TextChunk],
        embedder: &dyn Embedder,
        batch_size: usize,
    ) -> Result<()> {
        info!(
            "Building vector index from {} chunks (batch_size={})...",
            chunks.len(),
            batch_size
        );

        // Drop existing table if any
        let _ = self.db.drop_table("chunks", &[]).await;

        let schema = Self::schema(self.dim);
        let mut all_batches = Vec::new();

        for (i, batch_chunks) in chunks.chunks(batch_size).enumerate() {
            let texts: Vec<&str> = batch_chunks.iter().map(|c| c.text.as_str()).collect();
            let embeddings = embedder
                .embed(&texts)
                .await
                .context("Embedding batch failed")?;

            if embeddings.is_empty() {
                continue;
            }

            // Flatten embeddings for FixedSizeListArray
            let flat: Vec<f32> = embeddings.iter().flatten().copied().collect();
            let values = arrow_array::Float32Array::from(flat);
            let list_field = Arc::new(Field::new("item", DataType::Float32, true));
            let vector_array =
                FixedSizeListArray::try_new(list_field, self.dim as i32, Arc::new(values), None)?;

            let text_array = StringArray::from(
                batch_chunks
                    .iter()
                    .map(|c| c.text.as_str())
                    .collect::<Vec<_>>(),
            );
            let file_path_array = StringArray::from(
                batch_chunks
                    .iter()
                    .map(|c| c.metadata.file_path.as_str())
                    .collect::<Vec<_>>(),
            );
            let category_array = StringArray::from(
                batch_chunks
                    .iter()
                    .map(|c| c.metadata.category.as_str())
                    .collect::<Vec<_>>(),
            );
            let topic_array = StringArray::from(
                batch_chunks
                    .iter()
                    .map(|c| c.metadata.topic.as_str())
                    .collect::<Vec<_>>(),
            );
            let title_array = StringArray::from(
                batch_chunks
                    .iter()
                    .map(|c| c.metadata.title.as_str())
                    .collect::<Vec<_>>(),
            );
            let has_code_array = BooleanArray::from(
                batch_chunks
                    .iter()
                    .map(|c| c.metadata.has_code)
                    .collect::<Vec<_>>(),
            );

            let batch = RecordBatch::try_new(
                schema.clone(),
                vec![
                    Arc::new(vector_array),
                    Arc::new(text_array),
                    Arc::new(file_path_array),
                    Arc::new(category_array),
                    Arc::new(topic_array),
                    Arc::new(title_array),
                    Arc::new(has_code_array),
                ],
            )?;

            all_batches.push(batch);
            info!(
                "Embedded batch {}/{} ({} chunks)",
                i + 1,
                chunks.len().div_ceil(batch_size),
                batch_chunks.len()
            );
        }

        if all_batches.is_empty() {
            anyhow::bail!("No embeddings generated");
        }

        let batches = RecordBatchIterator::new(all_batches.into_iter().map(Ok), schema.clone());
        let table = self
            .db
            .create_table("chunks", Box::new(batches))
            .execute()
            .await
            .context("Failed to create LanceDB table")?;

        self.table = Some(table);
        info!("Vector index built successfully.");
        Ok(())
    }

    pub fn is_ready(&self) -> bool {
        self.table.is_some()
    }

    pub async fn search(
        &self,
        query_emb: &[f32],
        top_k: usize,
        category: Option<&str>,
    ) -> Result<Vec<SearchResult>> {
        let table = match &self.table {
            Some(t) => t,
            None => return Ok(Vec::new()),
        };

        let mut query = table
            .vector_search(query_emb)
            .context("Failed to build vector query")?
            .limit(if category.is_some() {
                top_k * CATEGORY_FILTER_MULTIPLIER
            } else {
                top_k
            });

        if let Some(cat) = category {
            query = query.only_if(format!("category = '{}'", cat.replace('\'', "''")));
        }

        let results = query.execute().await.context("Vector search failed")?;

        use futures::TryStreamExt;
        let batches: Vec<RecordBatch> = results.try_collect().await?;

        let mut search_results = Vec::new();
        for batch in &batches {
            let text_col = batch
                .column_by_name("text")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let fp_col = batch
                .column_by_name("file_path")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let cat_col = batch
                .column_by_name("category")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let topic_col = batch
                .column_by_name("topic")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let title_col = batch
                .column_by_name("title")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let code_col = batch
                .column_by_name("has_code")
                .and_then(|c| c.as_any().downcast_ref::<BooleanArray>());
            let dist_col = batch
                .column_by_name("_distance")
                .and_then(|c| c.as_any().downcast_ref::<arrow_array::Float32Array>());

            let (text_col, fp_col, cat_col, topic_col, title_col, code_col) =
                match (text_col, fp_col, cat_col, topic_col, title_col, code_col) {
                    (Some(a), Some(b), Some(c), Some(d), Some(e), Some(f)) => (a, b, c, d, e, f),
                    _ => continue,
                };

            for i in 0..batch.num_rows() {
                let score = dist_col
                    .map(|d| 1.0 / (1.0 + d.value(i) as f64))
                    .unwrap_or(0.0);
                search_results.push(SearchResult {
                    text: text_col.value(i).to_string(),
                    score,
                    metadata: SearchResultMetadata {
                        file_path: fp_col.value(i).to_string(),
                        category: cat_col.value(i).to_string(),
                        topic: topic_col.value(i).to_string(),
                        title: title_col.value(i).to_string(),
                        has_code: code_col.value(i),
                    },
                });

                if search_results.len() >= top_k {
                    break;
                }
            }
        }

        Ok(search_results)
    }
}
