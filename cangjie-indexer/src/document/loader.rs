use regex::Regex;
use std::sync::LazyLock;

use super::CODE_BLOCK_RE;
use crate::{DocData, DocMetadata};

// -- Title extraction --------------------------------------------------------

static H1_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^#\s+(.+)$").unwrap());

pub fn extract_title_from_content(content: &str) -> String {
    H1_RE
        .captures(content)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_default()
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
    let has_code = CODE_BLOCK_RE.is_match(&content);

    Some(DocData {
        text: content,
        metadata: DocMetadata {
            file_path: relative_path.to_string(),
            category: category.to_string(),
            topic: topic.to_string(),
            title,
            code_block_count: 0,
            has_code,
            chunk_id: String::new(),
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
