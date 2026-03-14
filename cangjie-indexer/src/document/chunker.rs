use text_splitter::MarkdownSplitter;
use tracing::info;

use super::{CODE_BLOCK_RE, HEADING_RE};
use crate::{DocData, TextChunk};

/// Parse the heading hierarchy active at each byte offset of the document.
///
/// Returns a sorted vec of `(byte_offset, level, heading_text)` for every
/// heading found in `text`.
fn parse_headings(text: &str) -> Vec<(usize, usize, String)> {
    HEADING_RE
        .captures_iter(text)
        .map(|cap| {
            let level = cap[1].len(); // number of '#' chars
            let title = cap[2].trim().to_string();
            let offset = cap.get(0).unwrap().start();
            (offset, level, title)
        })
        .collect()
}

/// Build a breadcrumb string like `[H1 > H2 > H3]` for the heading stack
/// that is active at `byte_offset` in the original document.
fn heading_breadcrumb(headings: &[(usize, usize, String)], byte_offset: usize) -> Option<String> {
    // Walk headings up to byte_offset, maintaining a stack by level.
    let mut stack: Vec<(usize, &str)> = Vec::new(); // (level, title)

    for (off, level, title) in headings {
        if *off > byte_offset {
            break;
        }
        // Pop everything at the same level or deeper.
        while stack.last().is_some_and(|(l, _)| *l >= *level) {
            stack.pop();
        }
        stack.push((*level, title.as_str()));
    }

    if stack.is_empty() {
        return None;
    }

    let crumb = stack
        .iter()
        .map(|(_, t)| *t)
        .collect::<Vec<_>>()
        .join(" > ");
    Some(format!("[{crumb}]"))
}

/// Find the byte offset of `chunk_text` inside `full_text`.
///
/// `MarkdownSplitter::chunks` returns string slices borrowed from the
/// original text, so we can compute the offset via pointer arithmetic.
fn chunk_byte_offset(full_text: &str, chunk_text: &str) -> usize {
    let full_start = full_text.as_ptr() as usize;
    let chunk_start = chunk_text.as_ptr() as usize;
    chunk_start.saturating_sub(full_start)
}

/// Count the number of fenced code blocks (``` ... ```) in a chunk.
fn count_code_blocks(text: &str) -> usize {
    CODE_BLOCK_RE.find_iter(text).count()
}

/// Split a document into chunks using markdown-aware splitting.
///
/// Each chunk is prefixed with its heading breadcrumb (e.g. `[H1 > H2]\n\n`)
/// to provide hierarchical context. Adjacent chunks share a small overlap
/// to preserve context across boundaries.
pub fn chunk_document(doc: &DocData, max_chunk_size: usize, overlap: usize) -> Vec<TextChunk> {
    let text = &doc.text;
    if text.is_empty() {
        return Vec::new();
    }

    let headings = parse_headings(text);
    let splitter = MarkdownSplitter::new(max_chunk_size);
    let raw_chunks: Vec<&str> = splitter.chunks(text).collect();

    if raw_chunks.is_empty() {
        return Vec::new();
    }
    let mut results = Vec::with_capacity(raw_chunks.len());

    for (idx, chunk) in raw_chunks.iter().enumerate() {
        let byte_off = chunk_byte_offset(text, chunk);

        // Build heading prefix.
        let prefix = heading_breadcrumb(&headings, byte_off);

        // Build overlap: take trailing chars from previous chunk.
        let overlap_text = if idx > 0 && overlap > 0 {
            let prev = raw_chunks[idx - 1];
            if prev.len() > overlap {
                let boundary = prev.floor_char_boundary(prev.len() - overlap);
                Some(&prev[boundary..])
            } else {
                Some(prev)
            }
        } else {
            None
        };

        // Assemble final chunk text.
        let mut assembled = String::new();
        if let Some(pfx) = &prefix {
            assembled.push_str(pfx);
            assembled.push_str("\n\n");
        }
        if let Some(ov) = overlap_text {
            assembled.push_str("...");
            assembled.push_str(ov);
            assembled.push_str("\n\n");
        }
        assembled.push_str(chunk);

        // Detect code blocks in the original chunk content.
        let code_block_count = count_code_blocks(chunk);
        let has_code = code_block_count > 0;

        let mut meta = doc.metadata.clone();
        meta.has_code = has_code;
        meta.code_block_count = code_block_count;
        meta.chunk_id = format!("{}#{}", doc.metadata.file_path, idx);

        results.push(TextChunk {
            text: assembled,
            metadata: meta,
        });
    }

    results
}

/// Strip indexing artifact prefixes added during chunk assembly.
///
/// Removes in order:
/// - `<context>...</context>\n\n` context summary prefix
/// - `[H1 > H2]\n\n` heading breadcrumb prefix
/// - `...<overlap text>\n\n` previous chunk overlap prefix
pub fn strip_chunk_artifacts(text: &str) -> &str {
    let mut s = text;

    // Strip <context>...</context>\n\n
    if let Some(rest) = s.strip_prefix("<context>") {
        if let Some(end) = rest.find("</context>\n\n") {
            s = &rest[end + "</context>\n\n".len()..];
        }
    }

    // Strip [breadcrumb]\n\n
    if let Some(rest) = s.strip_prefix('[') {
        if let Some(end) = rest.find("]\n\n") {
            s = &rest[end + "]\n\n".len()..];
        }
    }

    // Strip ...overlap\n\n
    if let Some(rest) = s.strip_prefix("...") {
        if let Some(end) = rest.find("\n\n") {
            s = &rest[end + "\n\n".len()..];
        }
    }

    s
}

pub async fn chunk_documents(
    docs: Vec<DocData>,
    max_chunk_size: usize,
    overlap: usize,
) -> Vec<TextChunk> {
    tokio::task::spawn_blocking(move || {
        let mut all_chunks = Vec::new();
        for doc in &docs {
            all_chunks.extend(chunk_document(doc, max_chunk_size, overlap));
        }
        info!(
            "Created {} chunks from {} documents.",
            all_chunks.len(),
            docs.len()
        );
        all_chunks
    })
    .await
    .expect("chunk_documents task panicked")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DocMetadata;

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
        let chunks = chunk_document(&doc, 500, 200);
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_chunk_small_document() {
        let doc = make_doc("Short text.");
        let chunks = chunk_document(&doc, 500, 200);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].text.contains("Short text."));
    }

    #[test]
    fn test_chunk_preserves_metadata() {
        let doc = make_doc("Some text content.");
        let chunks = chunk_document(&doc, 500, 200);
        assert_eq!(chunks[0].metadata.category, "test");
        assert_eq!(chunks[0].metadata.topic, "doc");
        assert_eq!(chunks[0].metadata.file_path, "test/doc.md");
    }

    #[test]
    fn test_chunk_detects_code() {
        let doc = make_doc("Some text\n\n```cangjie\nfunc main() {}\n```\n");
        let chunks = chunk_document(&doc, 5000, 200);
        assert!(chunks.iter().any(|c| c.metadata.has_code));
        assert!(chunks.iter().any(|c| c.metadata.code_block_count > 0));
    }

    #[test]
    fn test_chunk_large_document_splits() {
        let paragraph = "This is a paragraph of text. ".repeat(50);
        let text = format!("# Title\n\n{paragraph}\n\n## Section 2\n\n{paragraph}");
        let doc = make_doc(&text);
        let chunks = chunk_document(&doc, 500, 200);
        assert!(
            chunks.len() > 1,
            "Large document should be split into multiple chunks"
        );
    }

    #[test]
    fn test_chunk_id_generation() {
        let doc = make_doc("# Hello\n\nSome content here.");
        let chunks = chunk_document(&doc, 5000, 200);
        assert_eq!(chunks[0].metadata.chunk_id, "test/doc.md#0");
    }

    #[test]
    fn test_heading_breadcrumb_basic() {
        let headings = parse_headings("# Title\n\nText\n\n## Sub\n\nMore");
        assert_eq!(headings.len(), 2);
        assert_eq!(headings[0].1, 1); // H1
        assert_eq!(headings[1].1, 2); // H2
    }

    #[test]
    fn test_heading_prefix_injected() {
        let text = "# Main\n\n## Sub Section\n\nContent here.";
        let doc = make_doc(text);
        let chunks = chunk_document(&doc, 5000, 200);
        // The single chunk should contain the heading breadcrumb prefix
        assert!(
            chunks[0].text.contains("[Main]") || chunks[0].text.contains("[Main > Sub Section]"),
            "Chunk should have heading breadcrumb, got: {}",
            chunks[0].text
        );
    }

    #[test]
    fn test_heading_breadcrumb_hierarchy() {
        let headings = vec![
            (0, 1, "A".to_string()),
            (10, 2, "B".to_string()),
            (20, 3, "C".to_string()),
        ];
        let bc = heading_breadcrumb(&headings, 25);
        assert_eq!(bc.unwrap(), "[A > B > C]");
    }

    #[test]
    fn test_heading_breadcrumb_resets_on_same_level() {
        let headings = vec![
            (0, 1, "A".to_string()),
            (10, 2, "B".to_string()),
            (20, 2, "C".to_string()),
        ];
        // At offset 25, H2="C" should replace H2="B"
        let bc = heading_breadcrumb(&headings, 25);
        assert_eq!(bc.unwrap(), "[A > C]");
    }

    #[test]
    fn test_code_block_count() {
        let text = "```rust\nfoo\n```\n\nsome text\n\n```python\nbar\n```\n";
        assert_eq!(count_code_blocks(text), 2);
    }

    #[test]
    fn test_code_block_count_zero() {
        assert_eq!(count_code_blocks("no code here"), 0);
    }

    #[tokio::test]
    async fn test_chunk_documents_multiple() {
        let docs = vec![make_doc("Doc 1"), make_doc("Doc 2"), make_doc("Doc 3")];
        let chunks = chunk_documents(docs, 500, 200).await;
        assert_eq!(chunks.len(), 3);
    }

    #[test]
    fn test_strip_chunk_artifacts_breadcrumb() {
        let text = "[Main > Sub]\n\nActual content here.";
        assert_eq!(strip_chunk_artifacts(text), "Actual content here.");
    }

    #[test]
    fn test_strip_chunk_artifacts_overlap() {
        let text = "...previous chunk tail\n\nActual content here.";
        assert_eq!(strip_chunk_artifacts(text), "Actual content here.");
    }

    #[test]
    fn test_strip_chunk_artifacts_breadcrumb_and_overlap() {
        let text = "[H1 > H2]\n\n...overlap text\n\nActual content.";
        assert_eq!(strip_chunk_artifacts(text), "Actual content.");
    }

    #[test]
    fn test_strip_chunk_artifacts_context() {
        let text = "<context>This chunk is about variables.</context>\n\n[Main]\n\nContent.";
        assert_eq!(strip_chunk_artifacts(text), "Content.");
    }

    #[test]
    fn test_strip_chunk_artifacts_plain() {
        let text = "Plain text with no artifacts.";
        assert_eq!(strip_chunk_artifacts(text), "Plain text with no artifacts.");
    }
}
