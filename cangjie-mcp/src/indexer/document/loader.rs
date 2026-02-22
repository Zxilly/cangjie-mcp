use regex::Regex;
use std::sync::LazyLock;

use crate::indexer::{DocData, DocMetadata};

// -- Code Block --------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CodeBlock {
    pub language: String,
    pub code: String,
    pub context: String,
}

// -- Title extraction --------------------------------------------------------

static H1_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^#\s+(.+)$").unwrap());

pub fn extract_title_from_content(content: &str) -> String {
    H1_RE
        .captures(content)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_default()
}

// -- Code block extraction ---------------------------------------------------

static CODE_BLOCK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)```(\w*)\n(.*?)```").unwrap());

static HEADING_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)(^#{1,6}\s+.+$)").unwrap());

pub fn extract_code_blocks(content: &str) -> Vec<CodeBlock> {
    let mut blocks = Vec::new();

    for cap in CODE_BLOCK_RE.captures_iter(content) {
        let language = cap.get(1).map(|m| m.as_str()).unwrap_or("text").to_string();
        let language = if language.is_empty() {
            "text".to_string()
        } else {
            language
        };
        let code = cap
            .get(2)
            .map(|m| m.as_str().trim())
            .unwrap_or("")
            .to_string();

        let start_pos = cap.get(0).unwrap().start();
        let preceding = &content[..start_pos];
        let context = HEADING_RE
            .captures_iter(preceding)
            .last()
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();

        blocks.push(CodeBlock {
            language,
            code,
            context,
        });
    }

    blocks
}

// -- Document loading from files on disk -------------------------------------

pub fn load_document_from_content(
    content: String,
    relative_path: &str,
    category: &str,
    topic: &str,
) -> Option<DocData> {
    if content.trim().is_empty() {
        return None;
    }

    let title = extract_title_from_content(&content);
    let code_blocks = extract_code_blocks(&content);

    Some(DocData {
        text: content,
        metadata: DocMetadata {
            file_path: relative_path.to_string(),
            category: category.to_string(),
            topic: topic.to_string(),
            title,
            code_block_count: code_blocks.len(),
            has_code: !code_blocks.is_empty(),
        },
        doc_id: relative_path.to_string(),
    })
}

pub fn extract_metadata_from_relative_path(relative_path: &str) -> (String, String) {
    let parts: Vec<&str> = relative_path.split('/').collect();
    let category = if parts.len() > 1 {
        parts[0].to_string()
    } else {
        "general".to_string()
    };
    let topic = parts
        .last()
        .unwrap_or(&"")
        .strip_suffix(".md")
        .unwrap_or(parts.last().unwrap_or(&""))
        .to_string();
    (category, topic)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_title_from_content() {
        assert_eq!(extract_title_from_content("# Hello World"), "Hello World");
        assert_eq!(
            extract_title_from_content("# Hello World\nsome text"),
            "Hello World"
        );
        assert_eq!(extract_title_from_content("no title"), "");
        assert_eq!(extract_title_from_content("## Not H1"), "");
        assert_eq!(extract_title_from_content("# Trimmed  "), "Trimmed");
    }

    #[test]
    fn test_extract_title_multiline() {
        let content = "Some intro\n\n# Real Title\n\nBody text";
        assert_eq!(extract_title_from_content(content), "Real Title");
    }

    #[test]
    fn test_extract_code_blocks_basic() {
        let content = "## Section\n```cangjie\nfunc main() {}\n```\n";
        let blocks = extract_code_blocks(content);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].language, "cangjie");
        assert_eq!(blocks[0].code, "func main() {}");
        assert_eq!(blocks[0].context, "## Section");
    }

    #[test]
    fn test_extract_code_blocks_no_language() {
        let content = "```\nplain code\n```\n";
        let blocks = extract_code_blocks(content);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].language, "text");
    }

    #[test]
    fn test_extract_code_blocks_multiple() {
        let content = "# Title\n```rust\nlet x = 1;\n```\n## Other\n```python\nx = 1\n```\n";
        let blocks = extract_code_blocks(content);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].language, "rust");
        assert_eq!(blocks[1].language, "python");
        assert_eq!(blocks[1].context, "## Other");
    }

    #[test]
    fn test_extract_code_blocks_empty() {
        let blocks = extract_code_blocks("no code here");
        assert!(blocks.is_empty());
    }

    #[test]
    fn test_load_document_from_content_basic() {
        let content =
            "# Functions\nSome text about functions.\n```cangjie\nfunc foo() {}\n```\n".to_string();
        let doc = load_document_from_content(
            content.clone(),
            "syntax/functions.md",
            "syntax",
            "functions",
        );
        assert!(doc.is_some());
        let doc = doc.unwrap();
        assert_eq!(doc.metadata.title, "Functions");
        assert_eq!(doc.metadata.category, "syntax");
        assert_eq!(doc.metadata.topic, "functions");
        assert!(doc.metadata.has_code);
        assert_eq!(doc.metadata.code_block_count, 1);
    }

    #[test]
    fn test_load_document_from_content_empty() {
        let doc = load_document_from_content("".to_string(), "a/b.md", "a", "b");
        assert!(doc.is_none());

        let doc = load_document_from_content("   ".to_string(), "a/b.md", "a", "b");
        assert!(doc.is_none());
    }

    #[test]
    fn test_load_document_from_content_no_code() {
        let doc = load_document_from_content("# Title\nJust text.".to_string(), "a/b.md", "a", "b");
        let doc = doc.unwrap();
        assert!(!doc.metadata.has_code);
        assert_eq!(doc.metadata.code_block_count, 0);
    }

    #[test]
    fn test_extract_metadata_from_relative_path() {
        let (cat, topic) = extract_metadata_from_relative_path("syntax/functions.md");
        assert_eq!(cat, "syntax");
        assert_eq!(topic, "functions");

        let (cat, topic) = extract_metadata_from_relative_path("standalone.md");
        assert_eq!(cat, "general");
        assert_eq!(topic, "standalone");

        let (cat, topic) = extract_metadata_from_relative_path("syntax/sub/deep.md");
        assert_eq!(cat, "syntax");
        assert_eq!(topic, "deep");
    }
}
