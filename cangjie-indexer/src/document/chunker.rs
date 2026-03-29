use text_splitter::{CodeSplitter, MarkdownSplitter};
use tracing::info;

use cangjie_core::config::{
    CODE_DENSE_THRESHOLD, CODE_MIXED_THRESHOLD, DEFAULT_CODE_DENSE_CHARS, DEFAULT_CODE_MIXED_CHARS,
    DEFAULT_TEXT_HEAVY_CHARS,
};

use super::{CODE_BLOCK_RE, HEADING_RE};
use crate::{DocData, TextChunk};

/// Check if a language tag refers to Cangjie (empty tag is treated as Cangjie).
fn is_cangjie_lang(lang: &str) -> bool {
    lang.is_empty() || lang == "cangjie" || lang == "cj"
}

/// Calculate the ratio of code block content to total document text.
fn code_block_density(text: &str) -> f64 {
    if text.is_empty() {
        return 0.0;
    }
    let code_chars: usize = CODE_BLOCK_RE
        .captures_iter(text)
        .map(|cap| cap[2].len())
        .sum();
    code_chars as f64 / text.len() as f64
}

/// Determine the character budget for a document based on its code block density.
/// If `user_override` is set, returns that value directly.
fn compute_char_budget(text: &str, user_override: Option<usize>) -> usize {
    if let Some(max) = user_override {
        return max;
    }
    let density = code_block_density(text);
    if density > CODE_DENSE_THRESHOLD {
        DEFAULT_CODE_DENSE_CHARS
    } else if density > CODE_MIXED_THRESHOLD {
        DEFAULT_CODE_MIXED_CHARS
    } else {
        DEFAULT_TEXT_HEAVY_CHARS
    }
}

/// Split a Cangjie code block using tree-sitter if it exceeds the character budget.
/// Returns a vec of code strings (without fence markers).
fn split_code_block(code: &str, max_chars: usize) -> Vec<String> {
    if code.len() <= max_chars {
        return vec![code.to_string()];
    }
    match CodeSplitter::new(tree_sitter_cangjie::LANGUAGE, max_chars) {
        Ok(splitter) => {
            let chunks: Vec<String> = splitter.chunks(code).map(|s| s.to_string()).collect();
            if chunks.is_empty() {
                vec![code.to_string()]
            } else {
                chunks
            }
        }
        Err(e) => {
            tracing::warn!("CodeSplitter creation failed, keeping code block unsplit: {e}");
            vec![code.to_string()]
        }
    }
}

/// If a chunk contains Cangjie code blocks that exceed `max_chars`,
/// split them using CodeSplitter and return multiple sub-chunks.
/// Otherwise returns the chunk as-is in a single-element vec.
fn split_chunk_code_blocks(chunk: &str, max_chars: usize) -> Vec<String> {
    let mut result = Vec::new();
    let mut last_end = 0;
    let mut accum = String::new();
    let mut did_split = false;

    for cap in CODE_BLOCK_RE.captures_iter(chunk) {
        let full_match = cap.get(0).unwrap();
        let lang = &cap[1];
        let code_body = &cap[2];

        let needs_code_split = is_cangjie_lang(lang) && code_body.len() > max_chars;

        if needs_code_split {
            accum.push_str(&chunk[last_end..full_match.start()]);

            let code_chunks = split_code_block(code_body, max_chars);

            if code_chunks.len() == 1 {
                accum.push_str(&chunk[full_match.start()..full_match.end()]);
            } else {
                did_split = true;
                for (j, code_chunk) in code_chunks.iter().enumerate() {
                    let mut s = String::new();
                    if j == 0 && !accum.is_empty() {
                        s.push_str(&accum);
                        accum.clear();
                    }
                    s.push_str("```");
                    s.push_str(lang);
                    s.push('\n');
                    s.push_str(code_chunk);
                    if !code_chunk.ends_with('\n') {
                        s.push('\n');
                    }
                    s.push_str("```\n");
                    result.push(s);
                }
            }
        } else {
            accum.push_str(&chunk[last_end..full_match.end()]);
        }

        last_end = full_match.end();
    }

    if last_end < chunk.len() {
        accum.push_str(&chunk[last_end..]);
    }

    if !did_split {
        return vec![chunk.to_string()];
    }

    if !accum.is_empty() {
        if let Some(last) = result.last_mut() {
            last.push_str(&accum);
        }
    }

    result
}

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
    debug_assert!(
        chunk_start >= full_start && chunk_start <= full_start + full_text.len(),
        "chunk_text is not a slice of full_text"
    );
    chunk_start.saturating_sub(full_start)
}

/// Count the number of fenced code blocks (``` ... ```) in a chunk.
fn count_code_blocks(text: &str) -> usize {
    CODE_BLOCK_RE.find_iter(text).count()
}

/// Split a document into chunks using markdown-aware splitting with
/// dynamic content detection and two-stage code splitting.
///
/// Stage 1: Split by markdown structure using `MarkdownSplitter`.
/// Stage 2: For each chunk, split oversized Cangjie code blocks using `CodeSplitter`.
///
/// Each chunk is prefixed with its heading breadcrumb (e.g. `[H1 > H2]\n\n`)
/// to provide hierarchical context. Adjacent chunks share a small overlap
/// to preserve context across boundaries.
pub fn chunk_document(
    doc: &DocData,
    max_chunk_chars: Option<usize>,
    overlap_chars: usize,
) -> Vec<TextChunk> {
    let text = &doc.text;
    if text.is_empty() {
        return Vec::new();
    }

    let budget = compute_char_budget(text, max_chunk_chars);
    let headings = parse_headings(text);
    let splitter = MarkdownSplitter::new(budget);
    let raw_chunks: Vec<&str> = splitter.chunks(text).collect();

    if raw_chunks.is_empty() {
        return Vec::new();
    }

    let mut results = Vec::with_capacity(raw_chunks.len());
    let mut chunk_idx = 0;
    let mut prev_chunk_tail: Option<String> = None;

    for raw_chunk in &raw_chunks {
        let byte_off = chunk_byte_offset(text, raw_chunk);

        // Stage 2: split oversized code blocks
        let sub_chunks = split_chunk_code_blocks(raw_chunk, budget);

        // Build heading prefix.
        let prefix = heading_breadcrumb(&headings, byte_off);

        for sub_chunk in &sub_chunks {
            // Assemble final chunk text.
            let mut assembled = String::new();
            if let Some(pfx) = &prefix {
                assembled.push_str(pfx);
                assembled.push_str("\n\n");
            }
            if chunk_idx > 0 && overlap_chars > 0 {
                if let Some(ref tail) = prev_chunk_tail {
                    assembled.push_str("...");
                    assembled.push_str(tail);
                    assembled.push_str("\n\n");
                }
            }
            assembled.push_str(sub_chunk);

            let code_block_count = count_code_blocks(sub_chunk);

            let mut meta = doc.metadata.clone();
            meta.has_code = code_block_count > 0;
            meta.code_block_count = code_block_count;
            meta.chunk_id = format!("{}#{}", doc.metadata.file_path, chunk_idx);

            results.push(TextChunk {
                text: assembled,
                metadata: meta,
            });

            // Store only the overlap tail, not the full chunk.
            prev_chunk_tail = Some(if sub_chunk.len() > overlap_chars {
                let boundary = sub_chunk.floor_char_boundary(sub_chunk.len() - overlap_chars);
                sub_chunk[boundary..].to_string()
            } else {
                sub_chunk.clone()
            });
            chunk_idx += 1;
        }
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
    max_chunk_chars: Option<usize>,
    overlap_chars: usize,
) -> Vec<TextChunk> {
    tokio::task::spawn_blocking(move || {
        let mut all_chunks = Vec::new();
        for doc in &docs {
            all_chunks.extend(chunk_document(doc, max_chunk_chars, overlap_chars));
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
    use cangjie_core::config::DEFAULT_CHUNK_OVERLAP_CHARS;

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

    // ── code_block_density tests ───────────────────────────────────────────

    #[test]
    fn test_code_block_density_high() {
        let text = "intro\n\n```cangjie\nfunc main() {\n    let x = 1\n    let y = 2\n    let z = 3\n    println(x)\n    println(y)\n    println(z)\n}\n```\n";
        let density = code_block_density(text);
        assert!(density > 0.6, "Expected >0.6, got {density}");
    }

    #[test]
    fn test_code_block_density_zero() {
        let density = code_block_density("No code here at all.");
        assert_eq!(density, 0.0);
    }

    #[test]
    fn test_code_block_density_mixed() {
        let text = "Some explanation text that is fairly long and takes up space.\n\n```cangjie\nlet x = 1\n```\n\nMore text after.\n";
        let density = code_block_density(text);
        assert!(
            density > 0.0 && density < 0.6,
            "Expected mixed range, got {density}"
        );
    }

    // ── compute_char_budget tests ──────────────────────────────────────────

    #[test]
    fn test_compute_char_budget_override() {
        assert_eq!(compute_char_budget("anything", Some(999)), 999);
    }

    #[test]
    fn test_compute_char_budget_code_dense() {
        let text = "x\n\n```cangjie\nfunc main() {\n    let x = 1\n    let y = 2\n    let z = 3\n    println(x)\n    println(y)\n    println(z)\n}\n```\n";
        let budget = compute_char_budget(text, None);
        assert_eq!(budget, DEFAULT_CODE_DENSE_CHARS);
    }

    #[test]
    fn test_compute_char_budget_text_heavy() {
        let text = "This is a long document with no code at all. ".repeat(20);
        let budget = compute_char_budget(&text, None);
        assert_eq!(budget, DEFAULT_TEXT_HEAVY_CHARS);
    }

    // ── split_code_block tests ─────────────────────────────────────────────

    #[test]
    fn test_split_cangjie_code_block() {
        let code = "func a() { println(\"a\") }\nfunc b() { println(\"b\") }\nfunc c() { println(\"c\") }\n";
        let chunks = split_code_block(code, 40);
        assert!(
            chunks.len() > 1,
            "Code block should be split into multiple chunks, got {}",
            chunks.len()
        );
        for chunk in &chunks {
            assert!(!chunk.is_empty());
        }
    }

    #[test]
    fn test_split_cangjie_code_block_small_enough() {
        let code = "let x = 1\n";
        let chunks = split_code_block(code, 500);
        assert_eq!(chunks.len(), 1, "Small code block should not be split");
    }

    // ── chunk_document tests ───────────────────────────────────────────────

    #[test]
    fn test_chunk_document_dynamic_small_budget() {
        let doc = make_doc(
            "# Title\n\nSome text.\n\n```cangjie\nfunc main() {\n    println(\"hello\")\n}\n```\n\nMore text.",
        );
        let chunks = chunk_document(&doc, None, DEFAULT_CHUNK_OVERLAP_CHARS);
        assert!(!chunks.is_empty());
        for chunk in &chunks {
            assert!(!chunk.text.is_empty());
        }
    }

    #[test]
    fn test_chunk_empty_document() {
        let doc = make_doc("");
        let chunks = chunk_document(&doc, Some(500), 200);
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_chunk_small_document() {
        let doc = make_doc("Short text.");
        let chunks = chunk_document(&doc, Some(500), 200);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].text.contains("Short text."));
    }

    #[test]
    fn test_chunk_preserves_metadata() {
        let doc = make_doc("Some text content.");
        let chunks = chunk_document(&doc, Some(500), 200);
        assert_eq!(chunks[0].metadata.category, "test");
        assert_eq!(chunks[0].metadata.topic, "doc");
        assert_eq!(chunks[0].metadata.file_path, "test/doc.md");
    }

    #[test]
    fn test_chunk_detects_code() {
        let doc = make_doc("Some text\n\n```cangjie\nfunc main() {}\n```\n");
        let chunks = chunk_document(&doc, Some(5000), 200);
        assert!(chunks.iter().any(|c| c.metadata.has_code));
        assert!(chunks.iter().any(|c| c.metadata.code_block_count > 0));
    }

    #[test]
    fn test_chunk_large_document_splits() {
        let paragraph = "This is a paragraph of text. ".repeat(50);
        let text = format!("# Title\n\n{paragraph}\n\n## Section 2\n\n{paragraph}");
        let doc = make_doc(&text);
        let chunks = chunk_document(&doc, Some(500), 200);
        assert!(
            chunks.len() > 1,
            "Large document should be split into multiple chunks"
        );
    }

    #[test]
    fn test_chunk_id_generation() {
        let doc = make_doc("# Hello\n\nSome content here.");
        let chunks = chunk_document(&doc, Some(5000), 200);
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
        let chunks = chunk_document(&doc, Some(5000), 200);
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
        let chunks = chunk_documents(docs, Some(500), 200).await;
        assert_eq!(chunks.len(), 3);
    }

    #[test]
    fn test_split_chunk_code_blocks_preserves_text_between() {
        let chunk = "Before\n\n```cangjie\nfunc a() { println(\"a\") }\nfunc b() { println(\"b\") }\nfunc c() { println(\"c\") }\nfunc d() { println(\"d\") }\nfunc e() { println(\"e\") }\n```\n\nMiddle text\n\n```python\nprint('hello')\n```\n\nAfter text\n";
        let results = split_chunk_code_blocks(chunk, 40);
        // The middle text, python block, and after text should all be preserved
        let combined: String = results.join("");
        assert!(combined.contains("Middle text"), "Middle text was dropped");
        assert!(
            combined.contains("print('hello')"),
            "Python block was dropped"
        );
        assert!(combined.contains("After text"), "After text was dropped");
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
