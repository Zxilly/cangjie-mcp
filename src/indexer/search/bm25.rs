use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use jieba_rs::Jieba;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::*;
use tantivy::tokenizer::*;
use tantivy::{Index, IndexReader, IndexWriter, TantivyDocument};
use tracing::{info, warn};

use crate::config::{CATEGORY_FILTER_MULTIPLIER, INDEX_WRITER_HEAP_BYTES};
use crate::indexer::{SearchResult, SearchResultMetadata, TextChunk};

const TOKENIZER_NAME: &str = "jieba";

// -- Jieba Tokenizer for Tantivy ---------------------------------------------

#[derive(Clone)]
struct JiebaTokenizer {
    jieba: Arc<Jieba>,
}

impl JiebaTokenizer {
    fn new() -> Self {
        Self {
            jieba: Arc::new(Jieba::new()),
        }
    }
}

impl Tokenizer for JiebaTokenizer {
    type TokenStream<'a> = JiebaTokenStream;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        let lower = text.to_lowercase();
        let words = self.jieba.cut_for_search(&lower, true);
        let mut tokens = Vec::new();
        let mut offset = 0;

        for word in words {
            let word = word.trim();
            if word.is_empty() {
                continue;
            }
            tokens.push(Token {
                offset_from: offset,
                offset_to: offset + word.len(),
                position: tokens.len(),
                text: word.to_string(),
                position_length: 1,
            });
            offset += word.len();
        }

        JiebaTokenStream {
            tokens,
            index: 0,
            token: Token::default(),
        }
    }
}

struct JiebaTokenStream {
    tokens: Vec<Token>,
    index: usize,
    token: Token,
}

impl TokenStream for JiebaTokenStream {
    fn advance(&mut self) -> bool {
        if self.index < self.tokens.len() {
            self.token = self.tokens[self.index].clone();
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn token(&self) -> &Token {
        &self.token
    }

    fn token_mut(&mut self) -> &mut Token {
        &mut self.token
    }
}

// -- BM25 Store --------------------------------------------------------------

pub struct BM25Store {
    index_dir: PathBuf,
    index: Option<Index>,
    reader: Option<IndexReader>,
    schema: Schema,
    field_text: Field,
    field_file_path: Field,
    field_category: Field,
    field_topic: Field,
    field_title: Field,
    field_has_code: Field,
}

impl BM25Store {
    pub fn new(index_dir: PathBuf) -> Self {
        let mut schema_builder = Schema::builder();

        let text_options = TextOptions::default()
            .set_indexing_options(
                TextFieldIndexing::default()
                    .set_tokenizer(TOKENIZER_NAME)
                    .set_index_option(IndexRecordOption::WithFreqsAndPositions),
            )
            .set_stored();

        let field_text = schema_builder.add_text_field("text", text_options);
        let field_file_path = schema_builder.add_text_field("file_path", STRING | STORED);
        let field_category = schema_builder.add_text_field("category", STRING | STORED);
        let field_topic = schema_builder.add_text_field("topic", STRING | STORED);
        let field_title = schema_builder.add_text_field("title", STRING | STORED);
        let field_has_code = schema_builder.add_text_field("has_code", STRING | STORED);

        let schema = schema_builder.build();

        Self {
            index_dir,
            index: None,
            reader: None,
            schema,
            field_text,
            field_file_path,
            field_category,
            field_topic,
            field_title,
            field_has_code,
        }
    }

    fn register_tokenizer(index: &Index) {
        let tokenizer = JiebaTokenizer::new();
        index
            .tokenizers()
            .register(TOKENIZER_NAME, TextAnalyzer::builder(tokenizer).build());
    }

    pub fn is_indexed(&self) -> bool {
        self.index_dir.exists() && self.index_dir.join("meta.json").exists()
    }

    pub fn build_from_chunks(&mut self, chunks: &[TextChunk]) -> Result<()> {
        if chunks.is_empty() {
            warn!("No chunks provided for BM25 indexing.");
            return Ok(());
        }

        info!("Building BM25 index from {} chunks...", chunks.len());
        std::fs::create_dir_all(&self.index_dir)?;

        let index = Index::create_in_dir(&self.index_dir, self.schema.clone())
            .context("Failed to create tantivy index")?;
        Self::register_tokenizer(&index);

        let mut writer: IndexWriter = index
            .writer(INDEX_WRITER_HEAP_BYTES)
            .context("Failed to create index writer")?;

        for chunk in chunks {
            let mut doc = TantivyDocument::new();
            doc.add_text(self.field_text, &chunk.text);
            doc.add_text(self.field_file_path, &chunk.metadata.file_path);
            doc.add_text(self.field_category, &chunk.metadata.category);
            doc.add_text(self.field_topic, &chunk.metadata.topic);
            doc.add_text(self.field_title, &chunk.metadata.title);
            doc.add_text(
                self.field_has_code,
                if chunk.metadata.has_code {
                    "true"
                } else {
                    "false"
                },
            );
            writer.add_document(doc)?;
        }

        writer.commit()?;
        self.reader = Some(index.reader()?);
        self.index = Some(index);

        info!("BM25 index built and saved to {:?}", self.index_dir);
        Ok(())
    }

    pub fn load(&mut self) -> Result<bool> {
        if !self.is_indexed() {
            return Ok(false);
        }

        let index = Index::open_in_dir(&self.index_dir).context("Failed to open tantivy index")?;
        Self::register_tokenizer(&index);
        self.reader = Some(index.reader()?);
        self.index = Some(index);

        info!("BM25 index loaded from {:?}", self.index_dir);
        Ok(true)
    }

    pub fn search(
        &self,
        query: &str,
        top_k: usize,
        category: Option<&str>,
    ) -> Result<Vec<SearchResult>> {
        let index = match &self.index {
            Some(idx) => idx,
            None => return Ok(Vec::new()),
        };
        let reader = match &self.reader {
            Some(r) => r,
            None => return Ok(Vec::new()),
        };

        // Tokenize query with jieba for better CJK search
        let jieba = Jieba::new();
        let query_lower = query.to_lowercase();
        let tokens: Vec<&str> = jieba
            .cut_for_search(&query_lower, true)
            .into_iter()
            .filter(|w| !w.trim().is_empty())
            .collect();

        if tokens.is_empty() {
            return Ok(Vec::new());
        }

        let query_str = tokens.join(" ");
        let retrieve_k = if category.is_some() {
            top_k * CATEGORY_FILTER_MULTIPLIER
        } else {
            top_k
        };

        let searcher = reader.searcher();
        let query_parser = QueryParser::for_index(index, vec![self.field_text]);

        let tantivy_query = query_parser.parse_query(&query_str).unwrap_or_else(|_| {
            // Fallback: treat as simple term query
            Box::new(tantivy::query::AllQuery)
        });

        let top_docs = searcher
            .search(&tantivy_query, &TopDocs::with_limit(retrieve_k))
            .context("Search failed")?;

        let mut results = Vec::new();
        for (score, doc_addr) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_addr)?;

            let file_path = doc
                .get_first(self.field_file_path)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let cat = doc
                .get_first(self.field_category)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let topic = doc
                .get_first(self.field_topic)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let title = doc
                .get_first(self.field_title)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let has_code = doc
                .get_first(self.field_has_code)
                .and_then(|v| v.as_str())
                .unwrap_or("false")
                == "true";
            let text = doc
                .get_first(self.field_text)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // Apply category filter
            if let Some(filter_cat) = category {
                if cat != filter_cat {
                    continue;
                }
            }

            results.push(SearchResult {
                text,
                score: score as f64,
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

        Ok(results)
    }
}
