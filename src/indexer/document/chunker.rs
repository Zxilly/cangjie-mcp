use text_splitter::MarkdownSplitter;
use tracing::info;

use crate::indexer::{DocData, TextChunk};

/// Split a document into chunks using markdown-aware splitting.
pub fn chunk_document(doc: &DocData, max_chunk_size: usize) -> Vec<TextChunk> {
    let text = &doc.text;
    if text.is_empty() {
        return Vec::new();
    }

    let splitter = MarkdownSplitter::new(max_chunk_size);
    splitter
        .chunks(text)
        .map(|chunk| {
            let has_code = chunk.contains("```");
            let mut meta = doc.metadata.clone();
            meta.has_code = has_code;
            TextChunk {
                text: chunk.to_string(),
                metadata: meta,
            }
        })
        .collect()
}

pub fn chunk_documents(docs: &[DocData], max_chunk_size: usize) -> Vec<TextChunk> {
    let mut all_chunks = Vec::new();
    for doc in docs {
        all_chunks.extend(chunk_document(doc, max_chunk_size));
    }
    info!(
        "Created {} chunks from {} documents.",
        all_chunks.len(),
        docs.len()
    );
    all_chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::DocMetadata;

    fn make_doc(text: &str) -> DocData {
        DocData {
            text: text.to_string(),
            metadata: DocMetadata {
                file_path: "test/doc.md".to_string(),
                category: "test".to_string(),
                topic: "doc".to_string(),
                title: "Test".to_string(),
                ..Default::default()
            },
            doc_id: "test/doc.md".to_string(),
        }
    }

    #[test]
    fn test_chunk_empty_document() {
        let doc = make_doc("");
        let chunks = chunk_document(&doc, 500);
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_chunk_small_document() {
        let doc = make_doc("Short text.");
        let chunks = chunk_document(&doc, 500);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, "Short text.");
    }

    #[test]
    fn test_chunk_preserves_metadata() {
        let doc = make_doc("Some text content.");
        let chunks = chunk_document(&doc, 500);
        assert_eq!(chunks[0].metadata.category, "test");
        assert_eq!(chunks[0].metadata.topic, "doc");
        assert_eq!(chunks[0].metadata.file_path, "test/doc.md");
    }

    #[test]
    fn test_chunk_detects_code() {
        let doc = make_doc("Some text\n\n```cangjie\nfunc main() {}\n```\n");
        let chunks = chunk_document(&doc, 5000);
        assert!(chunks.iter().any(|c| c.metadata.has_code));
    }

    #[test]
    fn test_chunk_large_document_splits() {
        let paragraph = "This is a paragraph of text. ".repeat(50);
        let text = format!("# Title\n\n{paragraph}\n\n## Section 2\n\n{paragraph}");
        let doc = make_doc(&text);
        let chunks = chunk_document(&doc, 500);
        assert!(
            chunks.len() > 1,
            "Large document should be split into multiple chunks"
        );
    }

    #[test]
    fn test_chunk_documents_multiple() {
        let docs = vec![make_doc("Doc 1"), make_doc("Doc 2"), make_doc("Doc 3")];
        let chunks = chunk_documents(&docs, 500);
        assert_eq!(chunks.len(), 3);
    }
}
