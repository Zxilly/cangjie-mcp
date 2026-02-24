use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use arrow_array::{
    BooleanArray, FixedSizeListArray, Float32Array, RecordBatch, RecordBatchIterator, StringArray,
};
use lancedb::arrow::arrow_schema::{DataType, Field, Schema};
use lancedb::query::{ExecutableQuery, QueryBase};
use tracing::info;

use crate::config::CATEGORY_FILTER_MULTIPLIER;
use crate::indexer::embedding::Embedder;
use crate::indexer::{SearchResult, SearchResultMetadata, TextChunk};

const TABLE_NAME: &str = "chunks";
const COL_VECTOR: &str = "vector";
const COL_VECTOR_ITEM: &str = "item";
const COL_TEXT: &str = "text";
const COL_FILE_PATH: &str = "file_path";
const COL_CATEGORY: &str = "category";
const COL_TOPIC: &str = "topic";
const COL_TITLE: &str = "title";
const COL_HAS_CODE: &str = "has_code";
const COL_DISTANCE: &str = "_distance";

pub struct VectorStore {
    db: lancedb::Connection,
    table: Option<lancedb::Table>,
    dim: usize,
    schema: Arc<Schema>,
}

struct SearchBatchCols<'a> {
    text: &'a StringArray,
    file_path: &'a StringArray,
    category: &'a StringArray,
    topic: &'a StringArray,
    title: &'a StringArray,
    has_code: &'a BooleanArray,
    distance: Option<&'a Float32Array>,
}

impl VectorStore {
    pub async fn open(path: &Path, dim: usize) -> Result<Self> {
        let db = lancedb::connect(path.to_str().context("Invalid UTF-8 in path")?)
            .execute()
            .await
            .context("Failed to open LanceDB")?;

        let table = db.open_table(TABLE_NAME).execute().await.ok();
        let schema = Self::build_schema(dim);

        Ok(Self {
            db,
            table,
            dim,
            schema,
        })
    }

    fn build_schema(dim: usize) -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new(
                COL_VECTOR,
                DataType::FixedSizeList(
                    Arc::new(Field::new(COL_VECTOR_ITEM, DataType::Float32, true)),
                    dim as i32,
                ),
                false,
            ),
            Field::new(COL_TEXT, DataType::Utf8, false),
            Field::new(COL_FILE_PATH, DataType::Utf8, false),
            Field::new(COL_CATEGORY, DataType::Utf8, false),
            Field::new(COL_TOPIC, DataType::Utf8, false),
            Field::new(COL_TITLE, DataType::Utf8, false),
            Field::new(COL_HAS_CODE, DataType::Boolean, false),
        ]))
    }

    fn build_batch(
        schema: Arc<Schema>,
        dim: usize,
        batch_chunks: &[TextChunk],
        embeddings: &[Vec<f32>],
    ) -> Result<RecordBatch> {
        let flat: Vec<f32> = embeddings.iter().flatten().copied().collect();
        let values = Float32Array::from(flat);
        let list_field = Arc::new(Field::new(COL_VECTOR_ITEM, DataType::Float32, true));
        let vector_array =
            FixedSizeListArray::try_new(list_field, dim as i32, Arc::new(values), None)?;

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

        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(vector_array),
                Arc::new(text_array),
                Arc::new(file_path_array),
                Arc::new(category_array),
                Arc::new(topic_array),
                Arc::new(title_array),
                Arc::new(has_code_array),
            ],
        )
        .context("Failed to construct RecordBatch for vector store")
    }

    fn extract_search_columns(batch: &RecordBatch) -> Option<SearchBatchCols<'_>> {
        let text = batch
            .column_by_name(COL_TEXT)
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())?;
        let file_path = batch
            .column_by_name(COL_FILE_PATH)
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())?;
        let category = batch
            .column_by_name(COL_CATEGORY)
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())?;
        let topic = batch
            .column_by_name(COL_TOPIC)
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())?;
        let title = batch
            .column_by_name(COL_TITLE)
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())?;
        let has_code = batch
            .column_by_name(COL_HAS_CODE)
            .and_then(|c| c.as_any().downcast_ref::<BooleanArray>())?;
        let distance = batch
            .column_by_name(COL_DISTANCE)
            .and_then(|c| c.as_any().downcast_ref::<Float32Array>());

        Some(SearchBatchCols {
            text,
            file_path,
            category,
            topic,
            title,
            has_code,
            distance,
        })
    }

    fn append_search_results(
        batch: &RecordBatch,
        cols: SearchBatchCols<'_>,
        out: &mut Vec<SearchResult>,
        top_k: usize,
    ) {
        for i in 0..batch.num_rows() {
            let score = cols
                .distance
                .map(|d| 1.0 / (1.0 + d.value(i) as f64))
                .unwrap_or(0.0);
            out.push(SearchResult {
                text: cols.text.value(i).to_string(),
                score,
                metadata: SearchResultMetadata {
                    file_path: cols.file_path.value(i).to_string(),
                    category: cols.category.value(i).to_string(),
                    topic: cols.topic.value(i).to_string(),
                    title: cols.title.value(i).to_string(),
                    has_code: cols.has_code.value(i),
                },
            });

            if out.len() >= top_k {
                break;
            }
        }
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
        let _ = self.db.drop_table(TABLE_NAME, &[]).await;

        let schema = self.schema.clone();
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

            let batch = Self::build_batch(schema.clone(), self.dim, batch_chunks, &embeddings)?;

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
            .create_table(TABLE_NAME, Box::new(batches))
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
            if let Some(cols) = Self::extract_search_columns(batch) {
                Self::append_search_results(batch, cols, &mut search_results, top_k);
            }

            if search_results.len() >= top_k {
                break;
            }
        }

        Ok(search_results)
    }
}
