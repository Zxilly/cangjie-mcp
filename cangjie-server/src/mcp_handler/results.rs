use serde::{Deserialize, Serialize};

use cangjie_core::config::DEFAULT_TOP_K;
use rmcp::schemars;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SearchResultItem {
    pub content: String,
    pub score: f64,
    pub file_path: String,
    pub category: String,
    pub topic: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DocsSearchResult {
    pub items: Vec<SearchResultItem>,
    pub total: usize,
    pub count: usize,
    pub offset: usize,
    pub has_more: bool,
    pub next_offset: Option<usize>,
}

/// Format search results as compact Markdown for LLM consumption.
pub(crate) fn format_results_markdown(result: &DocsSearchResult) -> String {
    use std::fmt::Write;

    let mut out = String::new();
    let start = result.offset + 1;
    let end = result.offset + result.count;
    writeln!(
        out,
        "Found {} results (showing {start}-{end}):\n",
        result.total
    )
    .unwrap();

    for (i, item) in result.items.iter().enumerate() {
        let rank = result.offset + i + 1;
        writeln!(out, "---").unwrap();
        writeln!(
            out,
            "### [{rank}] {} ({}/{}) [score: {:.2}]\n",
            item.title, item.category, item.topic, item.score
        )
        .unwrap();
        writeln!(out, "{}\n", item.content).unwrap();
    }

    if result.has_more {
        if let Some(next) = result.next_offset {
            writeln!(out, "---").unwrap();
            writeln!(
                out,
                "_More results available. Use offset={next} to see next page._"
            )
            .unwrap();
        }
    }

    out
}

// ── Tool parameter types ────────────────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchDocsParams {
    /// Search query describing what you're looking for
    pub query: String,
    /// Optional category to filter results (e.g., 'cjpm', 'syntax', 'stdlib')
    #[serde(default)]
    pub category: Option<String>,
    /// Number of results to return (default: 5, max: 20)
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    /// Number of results to skip for pagination
    #[serde(default)]
    pub offset: usize,
    /// Filter by stdlib package name (e.g., 'std.collection', 'std.fs')
    #[serde(default)]
    pub package: Option<String>,
}

fn default_top_k() -> usize {
    DEFAULT_TOP_K
}
