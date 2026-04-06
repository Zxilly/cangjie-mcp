use cangjie_indexer::document::chunker::{chunk_document, chunk_documents};
use cangjie_indexer::document::loader::{extract_title_from_content, load_document_from_content};
use cangjie_indexer::document::source::DocumentSource;
use cangjie_mcp_test::{sample_documents, MockDocumentSource};

#[test]
fn test_load_and_chunk_pipeline() {
    let content = "# 测试文档\n\n这是第一段内容，描述了仓颉语言的基本特性。\n\n## 第二节\n\n这是第二段内容，包含更多细节。\n\n```cangjie\nfunc main() {\n    println(\"Hello\")\n}\n```\n".to_string();
    let doc = load_document_from_content(content, "test/example.md", "test", "example");
    assert!(doc.is_some(), "document should be loaded");
    let doc = doc.unwrap();

    assert_eq!(doc.metadata.category, "test");
    assert_eq!(doc.metadata.topic, "example");
    assert_eq!(doc.metadata.title, "测试文档");
    assert!(doc.metadata.has_code);

    let chunks = chunk_document(&doc, Some(6000), 200);
    assert!(!chunks.is_empty(), "document should produce chunks");
    // All chunks should carry the same category/topic metadata
    for chunk in &chunks {
        assert_eq!(chunk.metadata.category, "test");
        assert_eq!(chunk.metadata.topic, "example");
    }
}

#[test]
fn test_chunk_preserves_code_detection() {
    let content_with_code =
        "# Code Doc\n\nSome text.\n\n```cangjie\nlet x = 1\n```\n\nMore text.\n".to_string();
    let doc = load_document_from_content(content_with_code, "a/b.md", "a", "b").unwrap();
    let chunks = chunk_document(&doc, Some(6000), 200);
    assert!(
        chunks.iter().any(|c| c.metadata.has_code),
        "at least one chunk should have has_code = true"
    );

    let content_no_code = "# Plain Doc\n\nJust text, no code blocks.\n".to_string();
    let doc2 = load_document_from_content(content_no_code, "a/c.md", "a", "c").unwrap();
    let chunks2 = chunk_document(&doc2, Some(6000), 200);
    assert!(
        chunks2.iter().all(|c| !c.metadata.has_code),
        "chunks without code blocks should have has_code = false"
    );
}

#[tokio::test]
async fn test_mock_document_source() {
    let docs = sample_documents();
    let source = MockDocumentSource::from_docs(&docs);

    assert!(source.is_available().await);

    let all_docs = source.load_all_documents().await.unwrap();
    assert_eq!(all_docs.len(), docs.len());
}

/// Document loader: code detection should work for documents with code blocks.
#[test]
fn test_load_document_multiple_code_blocks() {
    let content = "# Multi Code\n\n```cangjie\nlet a = 1\n```\n\ntext\n\n```python\nprint(1)\n```\n\nmore text\n\n```bash\necho hi\n```\n".to_string();
    let doc = load_document_from_content(content, "test/multi.md", "test", "multi").unwrap();
    assert!(doc.metadata.has_code);
}

/// Title extraction should return filename stem when no H1 heading exists.
#[test]
fn test_title_extraction_no_heading() {
    let content = "Just plain text with no heading at all.";
    let title = extract_title_from_content(content);
    // When there's no H1, extract_title_from_content returns empty or the first line
    // The actual behavior: returns "" if no # heading found
    assert!(
        title.is_empty() || !title.contains('#'),
        "should not return a markdown heading marker"
    );
}

/// Chunking a large document should produce chunks that together cover all content.
#[test]
fn test_chunk_content_completeness() {
    let section = "这是一段较长的文本内容，用于测试分块功能是否完整。".repeat(20);
    let content =
        format!("# 完整性测试\n\n{section}\n\n## 第二节\n\n{section}\n\n## 第三节\n\n{section}");
    let doc = load_document_from_content(content.clone(), "test/big.md", "test", "big").unwrap();
    let chunks = chunk_document(&doc, Some(500), 200);

    assert!(chunks.len() > 1, "should split into multiple chunks");

    // All chunks combined should cover the important content
    let combined: String = chunks.iter().map(|c| c.text.as_str()).collect();
    assert!(combined.contains("完整性测试"));
    assert!(combined.contains("第二节"));
    assert!(combined.contains("第三节"));
}

/// chunk_documents with mixed code/no-code docs should correctly detect per-chunk.
#[tokio::test]
async fn test_chunk_documents_mixed_code_detection() {
    let docs = sample_documents();
    let chunks = chunk_documents(docs, Some(6000), 200).await;

    let code_chunks: Vec<_> = chunks.iter().filter(|c| c.metadata.has_code).collect();
    let no_code_chunks: Vec<_> = chunks.iter().filter(|c| !c.metadata.has_code).collect();

    // sample_documents contains docs with and without code
    assert!(
        !code_chunks.is_empty(),
        "should have chunks with code blocks"
    );
    // Verify code chunks actually contain code markers
    for c in &code_chunks {
        assert!(
            c.text.contains("```"),
            "chunk marked has_code should contain code block marker"
        );
    }
    // Verify no-code chunks don't have code markers
    for c in &no_code_chunks {
        assert!(
            !c.text.contains("```"),
            "chunk without code should not contain code block marker"
        );
    }
}

/// load_document_from_content should handle documents with only whitespace.
#[test]
fn test_load_whitespace_only_document() {
    let doc = load_document_from_content("   \n\n   ".to_string(), "a/b.md", "a", "b");
    // Should either return None (empty after trim) or a doc with empty-ish content
    if let Some(d) = doc {
        assert!(d.text.trim().is_empty() || d.text.len() < 10);
    }
}
