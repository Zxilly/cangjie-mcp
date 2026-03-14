pub mod chunker;
pub mod loader;
pub mod source;
pub mod summarizer;

use regex::Regex;
use std::sync::LazyLock;

/// Matches markdown headings H1-H6, capturing level (group 1) and title (group 2).
pub static HEADING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^(#{1,6})\s+(.+)$").unwrap());

/// Matches fenced code blocks with optional language tag.
/// Group 1: language, Group 2: code body.
pub static CODE_BLOCK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)```(\w*)\n(.*?)```").unwrap());
