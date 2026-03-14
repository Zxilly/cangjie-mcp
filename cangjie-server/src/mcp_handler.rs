use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[cfg(feature = "lsp")]
use crate::lsp_pool::LspPool;
use anyhow::Result;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{schemars, tool, tool_handler, tool_router, ServerHandler};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::info;
#[cfg(feature = "lsp")]
use tracing::warn;

use cangjie_core::config::{
    Settings, DEFAULT_TOP_K, MAX_SUGGESTIONS, MAX_TOP_K, MIN_TOP_K, PACKAGE_FETCH_MULTIPLIER,
    SIMILARITY_THRESHOLD,
};
use cangjie_core::prompts::get_prompt;
use cangjie_indexer::document::chunker::strip_chunk_artifacts;
use cangjie_indexer::document::loader::extract_code_blocks;
use cangjie_indexer::document::source::{DocumentSource, GitDocumentSource, RemoteDocumentSource};
use cangjie_indexer::search::{LocalSearchIndex, RemoteSearchIndex};
use cangjie_indexer::SearchResult;

// ── Output models ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CodeExample {
    pub language: String,
    pub code: String,
    pub context: String,
    pub source_topic: String,
    pub source_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SearchResultItem {
    pub content: String,
    pub score: f64,
    pub file_path: String,
    pub category: String,
    pub topic: String,
    pub title: String,
    pub has_code_examples: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_examples: Option<Vec<CodeExample>>,
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

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TopicInfo {
    pub name: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CategorySummary {
    pub topic_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topics: Option<Vec<TopicInfo>>,
}

/// Format search results as compact Markdown for LLM consumption.
fn format_results_markdown(result: &DocsSearchResult) -> String {
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
        if let Some(ref examples) = item.code_examples {
            for ex in examples {
                writeln!(out, "```{}\n{}\n```\n", ex.language, ex.code).unwrap();
            }
        }
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

/// Format a topic result as Markdown.
fn format_topic_markdown(
    file_path: &str,
    category: &str,
    topic: &str,
    title: &str,
    content: &str,
) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    writeln!(out, "# {title}\n").unwrap();
    writeln!(
        out,
        "**Topic:** {topic} | **Category:** {category} | **File:** {file_path}\n"
    )
    .unwrap();
    writeln!(out, "---\n").unwrap();
    write!(out, "{content}").unwrap();
    out
}

/// Format topics list as Markdown.
fn format_topics_list_markdown(
    categories: &HashMap<String, CategorySummary>,
    total_categories: usize,
    total_topics: usize,
) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    writeln!(
        out,
        "**{total_categories} categories, {total_topics} topics total**\n"
    )
    .unwrap();

    let mut sorted_cats: Vec<_> = categories.iter().collect();
    sorted_cats.sort_by_key(|(name, _)| (*name).clone());

    for (cat, summary) in sorted_cats {
        match &summary.topics {
            Some(topics) => {
                writeln!(out, "### {cat} ({} topics)\n", summary.topic_count).unwrap();
                for t in topics {
                    if t.title.is_empty() {
                        writeln!(out, "- {}", t.name).unwrap();
                    } else {
                        writeln!(out, "- **{}** — {}", t.name, t.title).unwrap();
                    }
                }
                writeln!(out).unwrap();
            }
            None => {
                writeln!(out, "- **{cat}** ({} topics)", summary.topic_count).unwrap();
            }
        }
    }

    out
}

// ── Internal state ──────────────────────────────────────────────────────────

#[derive(Clone)]
enum SearchBackend {
    Local(Arc<LocalSearchIndex>),
    Remote(Arc<RemoteSearchIndex>),
}

struct InnerState {
    search: SearchBackend,
    docs: Arc<dyn DocumentSource>,
}

// ── MCP Server ──────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct CangjieServer {
    state: Arc<RwLock<Option<InnerState>>>,
    settings: Settings,
    tool_router: ToolRouter<Self>,
    #[cfg(feature = "lsp")]
    lsp_pool: Option<Arc<LspPool>>,
}

impl CangjieServer {
    #[cfg(feature = "lsp")]
    fn log_lsp_startup_status() {
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let env_cangjie_home = std::env::var("CANGJIE_HOME")
            .ok()
            .filter(|v| !v.trim().is_empty());

        info!(
            "LSP startup check: cwd={}, env_CANGJIE_HOME={}",
            cwd.display(),
            env_cangjie_home.as_deref().unwrap_or("<unset>")
        );

        match cangjie_lsp::detect_settings(Some(cwd.clone())) {
            Some(lsp_settings) => {
                let lsp_server_path = lsp_settings.lsp_server_path();
                let validation_errors = lsp_settings.validate();
                info!(
                    "LSP startup check: enabled=true, workspace={}, sdk_path={}, lsp_server={}",
                    lsp_settings.workspace_path.display(),
                    lsp_settings.sdk_path.display(),
                    lsp_server_path.display()
                );
                if validation_errors.is_empty() {
                    info!("LSP startup check: settings validation passed");
                } else {
                    warn!(
                        "LSP startup check: settings validation warnings: {}",
                        validation_errors.join("; ")
                    );
                }
            }
            None => {
                warn!(
                    "LSP startup check: enabled=false, CANGJIE_HOME not found in env or .vscode/settings.json terminal.integrated.env*"
                );
            }
        }
    }

    fn docs_tool_router() -> ToolRouter<Self> {
        ToolRouter::<Self>::new()
            .with_route((Self::search_docs_tool_attr(), Self::search_docs))
            .with_route((Self::get_topic_tool_attr(), Self::get_topic))
            .with_route((Self::list_topics_tool_attr(), Self::list_topics))
    }

    fn build_tool_router() -> ToolRouter<Self> {
        let router = Self::docs_tool_router();
        #[cfg(feature = "lsp")]
        let router = {
            let mut router = router;
            if cangjie_lsp::is_available() {
                router.merge(Self::lsp_tool_router());
            }
            router
        };
        router
    }

    pub fn new(settings: Settings) -> Self {
        Self {
            state: Arc::new(RwLock::new(None)),
            settings,
            tool_router: Self::build_tool_router(),
            #[cfg(feature = "lsp")]
            lsp_pool: None,
        }
    }

    /// Create a server with an LSP pool for daemon mode.
    /// LSP clients are created on demand per workspace directory.
    #[cfg(feature = "lsp")]
    pub fn with_lsp_pool(settings: Settings, idle_timeout: std::time::Duration) -> Self {
        Self {
            state: Arc::new(RwLock::new(None)),
            settings,
            tool_router: Self::build_tool_router(),
            lsp_pool: Some(Arc::new(LspPool::new(idle_timeout))),
        }
    }

    /// Get a reference to the LSP pool (if in daemon mode).
    #[cfg(feature = "lsp")]
    pub fn lsp_pool(&self) -> Option<&Arc<LspPool>> {
        self.lsp_pool.as_ref()
    }

    /// Create a `CangjieServer` with pre-initialized state (for testing).
    #[doc(hidden)]
    pub fn with_local_state(
        settings: Settings,
        search: LocalSearchIndex,
        docs: Box<dyn DocumentSource>,
    ) -> Self {
        let inner = InnerState {
            search: SearchBackend::Local(Arc::new(search)),
            docs: Arc::from(docs),
        };
        Self {
            state: Arc::new(RwLock::new(Some(inner))),
            settings,
            tool_router: Self::build_tool_router(),
            #[cfg(feature = "lsp")]
            lsp_pool: None,
        }
    }

    /// Initialize the server (clone repo, build index, etc.)
    pub async fn initialize(&self) -> Result<()> {
        let settings = self.settings.clone();
        info!("Initializing index...");

        #[cfg(feature = "lsp")]
        {
            if self.lsp_pool.is_some() {
                info!("LSP startup: using pool mode (clients created on demand per workspace)");
            } else {
                Self::log_lsp_startup_status();
                let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
                if let Some(lsp_settings) = cangjie_lsp::detect_settings(Some(cwd)) {
                    if cangjie_lsp::init(lsp_settings).await {
                        info!("LSP startup: initialization completed");
                    } else {
                        warn!("LSP startup: initialization failed, LSP tools will be unavailable");
                    }
                } else {
                    warn!(
                        "LSP startup: skipped initialization because CANGJIE_HOME is not configured"
                    );
                }
            }
        }

        let (search, index_info) = if let Some(ref url) = settings.server_url {
            let remote = RemoteSearchIndex::new(&settings, url)?;
            let info = remote.init().await?;
            (SearchBackend::Remote(Arc::new(remote)), info)
        } else {
            let mut local = LocalSearchIndex::new(settings.clone()).await;
            let info = local.init().await?;
            (SearchBackend::Local(Arc::new(local)), info)
        };

        cangjie_core::config::log_startup_info(&settings, &index_info);

        let docs: Arc<dyn DocumentSource> = if let Some(ref url) = settings.server_url {
            Arc::new(RemoteDocumentSource::new(&settings, url)?)
        } else {
            let repo_dir = settings.docs_repo_dir();
            let lang = index_info.lang;
            let git_source =
                tokio::task::spawn_blocking(move || GitDocumentSource::new(repo_dir, lang))
                    .await??;
            Arc::new(git_source)
        };

        let inner = InnerState { search, docs };
        *self.state.write().await = Some(inner);
        info!("Initialization complete — tools are ready.");
        Ok(())
    }

    async fn do_search(
        &self,
        query: &str,
        top_k: usize,
        category: Option<&str>,
    ) -> Result<Vec<SearchResult>> {
        let search = {
            let state = self.state.read().await;
            let inner = state
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Server not initialized"))?;
            inner.search.clone()
        };

        match search {
            SearchBackend::Local(local) => local.query(query, top_k, category, true).await,
            SearchBackend::Remote(remote) => remote.query(query, top_k, category, true).await,
        }
    }

    fn has_package(result: &SearchResult, package: &str) -> bool {
        result.text.contains(package) || result.text.contains(&format!("import {package}"))
    }

    fn query_terms(query: &str) -> Vec<String> {
        let jieba = &**cangjie_indexer::search::GLOBAL_JIEBA;
        let lower = query.to_lowercase();
        let mut terms: Vec<String> = jieba
            .cut_for_search(&lower, true)
            .into_iter()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();

        for token in lower
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|t| !t.is_empty())
        {
            if !terms.iter().any(|t| t == token) {
                terms.push(token.to_string());
            }
        }

        terms
    }

    fn lexical_boost(query_terms: &[String], query_lc: &str, item: &SearchResult) -> f64 {
        let topic = item.metadata.topic.to_lowercase();
        let title = item.metadata.title.to_lowercase();
        let path = item.metadata.file_path.to_lowercase();
        let text = item.text.to_lowercase();
        let mut boost = 0.0;

        for term in query_terms {
            if topic == *term {
                boost += 8.0;
            } else if topic.contains(term) {
                boost += 5.0;
            }

            if title == *term {
                boost += 6.0;
            } else if title.contains(term) {
                boost += 4.0;
            }

            if path.contains(term) {
                boost += 2.0;
            }

            if text.contains(term) {
                boost += 1.5;
            }
        }

        if !query_lc.is_empty() {
            if topic.contains(query_lc) {
                boost += 6.0;
            }
            if title.contains(query_lc) {
                boost += 5.0;
            }
            if text.contains(query_lc) {
                boost += 2.0;
            }
        }

        boost
    }

    fn rerank_and_dedup_results(
        results: Vec<SearchResult>,
        query: &str,
        top_k: usize,
        offset: usize,
    ) -> Vec<SearchResult> {
        /// Maximum possible boost per query term (topic exact 8 + title exact 6 + text 1.5)
        const MAX_BOOST_PER_TERM: f64 = 15.5;
        /// Maximum whole-query boost (topic 6 + title 5 + text 2)
        const MAX_WHOLE_QUERY_BOOST: f64 = 13.0;
        /// Weight cap for lexical boost in final score
        const BOOST_WEIGHT: f64 = 0.3;

        let query_terms = Self::query_terms(query);
        let query_lc = query.to_lowercase();
        let max_possible = query_terms.len() as f64 * MAX_BOOST_PER_TERM + MAX_WHOLE_QUERY_BOOST;
        let mut scored: Vec<(SearchResult, f64)> = results
            .into_iter()
            .map(|r| {
                let raw_boost = Self::lexical_boost(&query_terms, &query_lc, &r);
                let normalized_boost = if max_possible > 0.0 {
                    raw_boost / max_possible
                } else {
                    0.0
                };
                let adjusted = r.score + BOOST_WEIGHT * normalized_boost;
                (r, adjusted)
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let per_doc_limit = if top_k <= 3 { 1 } else { 2 };
        let limit = offset + top_k + 1;

        // Strong duplicate suppression for near-identical snippets.
        let mut seen_text_keys: HashSet<String> = HashSet::new();
        let mut candidates: Vec<(SearchResult, f64)> = Vec::new();
        for (result, adjusted) in scored {
            let text_key = result
                .text
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase();
            if !seen_text_keys.insert(text_key) {
                continue;
            }
            candidates.push((result, adjusted));
        }

        // Phase 1: maximize document coverage (at most one per document).
        let mut selected: Vec<(SearchResult, f64)> = Vec::new();
        let mut per_doc_count: HashMap<String, usize> = HashMap::new();
        for (result, adjusted) in &candidates {
            if selected.len() >= limit {
                break;
            }
            let key = result.metadata.file_path.clone();
            if per_doc_count.get(&key).copied().unwrap_or(0) == 0 {
                selected.push((result.clone(), *adjusted));
                per_doc_count.insert(key, 1);
            }
        }

        // Phase 2: backfill with additional high-scoring snippets up to per-doc cap.
        for (result, adjusted) in candidates {
            if selected.len() >= limit {
                break;
            }
            let key = result.metadata.file_path.clone();
            let count = per_doc_count.get(&key).copied().unwrap_or(0);
            if count >= per_doc_limit {
                continue;
            }
            if count == 0 {
                continue;
            }
            selected.push((result, adjusted));
            per_doc_count.insert(key, count + 1);
        }

        selected.into_iter().map(|(result, _)| result).collect()
    }

    async fn build_topic_category_map(
        docs: &Arc<dyn DocumentSource>,
    ) -> Result<HashMap<String, Vec<String>>> {
        let categories = docs.get_categories().await?;
        let mut mapping: HashMap<String, Vec<String>> = HashMap::new();
        for cat in categories {
            let topics = docs.get_topics_in_category(&cat).await?;
            for topic in topics {
                mapping.entry(topic).or_default().push(cat.clone());
            }
        }
        for cats in mapping.values_mut() {
            cats.sort();
            cats.dedup();
        }
        Ok(mapping)
    }

    fn topic_display_with_categories(
        topic: &str,
        topic_category_map: &HashMap<String, Vec<String>>,
    ) -> String {
        match topic_category_map.get(topic) {
            Some(cats) if !cats.is_empty() => {
                if cats.len() == 1 {
                    format!("{topic} (in {})", cats[0])
                } else {
                    format!("{topic} (in {})", cats.join(", "))
                }
            }
            _ => topic.to_string(),
        }
    }
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
    /// Whether to extract code examples from results (default: true)
    #[serde(default = "default_true")]
    pub extract_code: bool,
    /// Filter by stdlib package name (e.g., 'std.collection', 'std.fs')
    #[serde(default)]
    pub package: Option<String>,
}

fn default_top_k() -> usize {
    DEFAULT_TOP_K
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetTopicParams {
    /// Topic name - the documentation file name without .md extension
    pub topic: String,
    /// Optional category to narrow the search (e.g., 'syntax', 'stdlib')
    #[serde(default)]
    pub category: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListTopicsParams {
    /// Optional category to filter by (e.g., 'cjpm', 'syntax')
    #[serde(default)]
    pub category: Option<String>,
    /// Whether to include full topic lists per category (default: true). Set to false for a compact summary with only category names and counts.
    #[serde(default = "default_true")]
    pub detail: bool,
}

fn default_true() -> bool {
    true
}

// ── Tool implementations ────────────────────────────────────────────────────

#[tool_router]
impl CangjieServer {
    #[tool(
        name = "cangjie_lsp",
        description = "Unified Cangjie LSP entry point. Use operation to run definition, references, hover, document_symbol, diagnostics, workspace_symbol, incoming_calls, outgoing_calls, type hierarchy, rename, and completion.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    pub async fn lsp(
        &self,
        Parameters(params): Parameters<crate::lsp_tools::LspRequest>,
        meta: rmcp::model::Meta,
    ) -> String {
        // Extract working directory from _meta header
        let working_dir = meta
            .0
            .get(crate::lsp_tools::META_WORKING_DIRECTORY)
            .and_then(|v| v.as_str())
            .map(std::path::PathBuf::from);

        #[cfg(feature = "lsp")]
        {
            crate::lsp_tools::execute_lsp_request(params, self.lsp_pool.as_deref(), working_dir)
                .await
        }
        #[cfg(not(feature = "lsp"))]
        {
            let _ = working_dir;
            crate::lsp_tools::execute_lsp_request(params).await
        }
    }

    #[tool(
        name = "cangjie_search_docs",
        description = "Search Cangjie documentation using semantic search. Performs similarity search across all indexed documentation. Returns matching sections ranked by relevance with code examples and pagination support (use offset/top_k). Supports filtering by category (e.g. 'stdlib', 'syntax') and stdlib package name (e.g. 'std.collection', 'std.fs').",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    pub async fn search_docs(&self, Parameters(params): Parameters<SearchDocsParams>) -> String {
        let top_k = params.top_k.clamp(MIN_TOP_K, MAX_TOP_K);
        let category = params.category.as_deref().filter(|s| !s.is_empty());
        let package = params.package.as_deref().filter(|s| !s.is_empty());

        // Retrieve extra candidates so lexical reranking and deduplication still
        // has enough headroom for pagination.
        let dedup_fetch_multiplier = 4;
        let fetch_multiplier = if package.is_some() {
            PACKAGE_FETCH_MULTIPLIER
        } else {
            1
        };
        let fetch_count = (params.offset + top_k + 1) * fetch_multiplier * dedup_fetch_multiplier;

        let results = match self.do_search(&params.query, fetch_count, category).await {
            Ok(r) => r,
            Err(e) => return format!("Search error: {e}"),
        };

        let mut results =
            Self::rerank_and_dedup_results(results, &params.query, top_k, params.offset);

        if let Some(pkg) = package {
            results.retain(|r| Self::has_package(r, pkg));
        }

        let total = results.len();
        let paginated: Vec<_> = results
            .into_iter()
            .skip(params.offset)
            .take(top_k)
            .collect();
        let has_more = total > params.offset + top_k;

        let items: Vec<SearchResultItem> = paginated
            .into_iter()
            .map(|r| {
                let code_examples = if params.extract_code {
                    let blocks = extract_code_blocks(&r.text);
                    Some(
                        blocks
                            .into_iter()
                            .map(|b| CodeExample {
                                language: b.language,
                                code: b.code,
                                context: b.context,
                                source_topic: r.metadata.topic.clone(),
                                source_file: r.metadata.file_path.clone(),
                            })
                            .collect(),
                    )
                } else {
                    None
                };
                SearchResultItem {
                    content: strip_chunk_artifacts(&r.text).to_string(),
                    score: r.score,
                    file_path: r.metadata.file_path,
                    category: r.metadata.category,
                    topic: r.metadata.topic,
                    title: r.metadata.title,
                    has_code_examples: r.metadata.has_code,
                    code_examples,
                }
            })
            .collect();

        let count = items.len();
        let result = DocsSearchResult {
            items,
            total,
            count,
            offset: params.offset,
            has_more,
            next_offset: if has_more {
                Some(params.offset + count)
            } else {
                None
            },
        };

        format_results_markdown(&result)
    }

    #[tool(
        name = "cangjie_get_topic",
        description = "Get complete documentation for a specific topic. Retrieves the full content of a documentation file by topic name. Use cangjie_list_topics first to discover available topic names.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    pub async fn get_topic(&self, Parameters(params): Parameters<GetTopicParams>) -> String {
        let docs = {
            let state = self.state.read().await;
            match state.as_ref() {
                Some(s) => s.docs.clone(),
                None => return "Server not initialized".to_string(),
            }
        };

        let topic = &params.topic;
        let category = params.category.as_deref().filter(|s| !s.is_empty());

        let doc = docs.get_document_by_topic(topic, category).await;

        match doc {
            Ok(Some(doc)) => format_topic_markdown(
                &doc.metadata.file_path,
                &doc.metadata.category,
                &doc.metadata.topic,
                &doc.metadata.title,
                &doc.text,
            ),
            Ok(None) => {
                // If the provided category is wrong, fallback to cross-category search.
                if category.is_some() {
                    match docs.get_document_by_topic(topic, None).await {
                        Ok(Some(doc)) => {
                            return format_topic_markdown(
                                &doc.metadata.file_path,
                                &doc.metadata.category,
                                &doc.metadata.topic,
                                &doc.metadata.title,
                                &doc.text,
                            );
                        }
                        Ok(None) => {}
                        Err(e) => return format!("Error retrieving topic '{topic}': {e}"),
                    }
                }

                let topic_category_map = Self::build_topic_category_map(&docs)
                    .await
                    .unwrap_or_default();
                let all_topics = docs.get_all_topic_names().await.unwrap_or_default();
                let mut suggestions: Vec<(String, f64)> = all_topics
                    .iter()
                    .map(|t| {
                        let sim = strsim::jaro_winkler(topic, t);
                        (t.clone(), sim)
                    })
                    .filter(|(_, sim)| *sim > SIMILARITY_THRESHOLD)
                    .collect();
                suggestions
                    .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                suggestions.truncate(MAX_SUGGESTIONS);

                let mut msg = format!("Topic '{topic}' not found.");
                if let Some(cat) = category {
                    if let Some(cats) = topic_category_map.get(topic) {
                        if !cats.iter().any(|c| c == cat) {
                            msg.push_str(&format!(
                                "\nTopic '{topic}' exists in category: {}.",
                                cats.join(", ")
                            ));
                        }
                    }
                }
                if !suggestions.is_empty() {
                    let names: Vec<String> = suggestions
                        .iter()
                        .map(|(name, _)| {
                            Self::topic_display_with_categories(name, &topic_category_map)
                        })
                        .collect();
                    msg.push_str(&format!("\nDid you mean: {}?", names.join(", ")));
                }
                msg
            }
            Err(e) => {
                format!("Error retrieving topic '{topic}': {e}")
            }
        }
    }

    #[tool(
        name = "cangjie_list_topics",
        description = "List available documentation topics organized by category. Returns all documentation topics with names, optionally filtered by category. Use this to discover topic names for use with cangjie_get_topic. Set detail=false for a compact summary with only category names and topic counts.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    pub async fn list_topics(&self, Parameters(params): Parameters<ListTopicsParams>) -> String {
        let docs = {
            let state = self.state.read().await;
            match state.as_ref() {
                Some(s) => s.docs.clone(),
                None => return "Server not initialized".to_string(),
            }
        };

        let filter_category = params.category.as_deref().filter(|s| !s.is_empty());

        if let Some(cat) = filter_category {
            let all_cats = docs.get_categories().await.unwrap_or_default();
            if !all_cats.contains(&cat.to_string()) {
                return format!(
                    "Category '{}' not found.\n\nAvailable categories: {}",
                    cat,
                    all_cats.join(", ")
                );
            }
        }

        let categories_to_list = if let Some(cat) = filter_category {
            vec![cat.to_string()]
        } else {
            docs.get_categories().await.unwrap_or_default()
        };

        let mut categories = HashMap::new();
        for cat in &categories_to_list {
            let topics = docs.get_topics_in_category(cat).await.unwrap_or_default();
            if !topics.is_empty() {
                let summary = if params.detail {
                    let titles = docs.get_topic_titles(cat).await.unwrap_or_default();
                    let infos: Vec<TopicInfo> = topics
                        .iter()
                        .map(|t| TopicInfo {
                            name: t.clone(),
                            title: titles.get(t).cloned().unwrap_or_default(),
                        })
                        .collect();
                    CategorySummary {
                        topic_count: infos.len(),
                        topics: Some(infos),
                    }
                } else {
                    CategorySummary {
                        topic_count: topics.len(),
                        topics: None,
                    }
                };
                categories.insert(cat.clone(), summary);
            }
        }

        let total_topics: usize = categories.values().map(|v| v.topic_count).sum();

        format_topics_list_markdown(&categories, categories.len(), total_topics)
    }

    // ── LSP Tools ──────────────────────────────────────────────────────────
}

// ── ServerHandler impl ──────────────────────────────────────────────────────

#[tool_handler]
impl ServerHandler for CangjieServer {
    fn get_info(&self) -> ServerInfo {
        #[cfg(feature = "lsp")]
        let lsp_enabled = cangjie_lsp::is_available();
        #[cfg(not(feature = "lsp"))]
        let lsp_enabled = false;

        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("cangjie-mcp", cangjie_core::VERSION))
            .with_instructions(get_prompt(lsp_enabled))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cangjie_indexer::{SearchResult, SearchResultMetadata};

    #[test]
    fn test_has_package_direct_match() {
        let result = SearchResult {
            text: "This text mentions std.collection directly".to_string(),
            score: 1.0,
            metadata: SearchResultMetadata::default(),
        };
        assert!(CangjieServer::has_package(&result, "std.collection"));
    }

    #[test]
    fn test_has_package_import_match() {
        let result = SearchResult {
            text: "You can use import std.fs to access filesystem APIs".to_string(),
            score: 1.0,
            metadata: SearchResultMetadata::default(),
        };
        assert!(CangjieServer::has_package(&result, "std.fs"));
    }

    #[test]
    fn test_has_package_no_match() {
        let result = SearchResult {
            text: "This text has nothing relevant to any package".to_string(),
            score: 1.0,
            metadata: SearchResultMetadata::default(),
        };
        assert!(!CangjieServer::has_package(&result, "std.collection"));
    }

    #[test]
    fn test_get_info_without_cangjie_home() {
        temp_env::with_var("CANGJIE_HOME", None::<&str>, || {
            let settings = Settings {
                chunk_max_size: 6000,
                data_dir: std::path::PathBuf::from("/tmp/test-info"),
                openai_base_url: "https://api.example.com".to_string(),
                openai_model: "test".to_string(),
                ..Settings::default()
            };

            let server = CangjieServer::new(settings);
            let info = server.get_info();

            assert!(
                info.instructions.is_some(),
                "instructions should be present"
            );
            let instructions = info.instructions.unwrap();
            assert!(!instructions.is_empty(), "instructions should not be empty");

            let tools = server.tool_router.list_all();
            let tool_names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
            assert!(
                !tool_names.iter().any(|n| n == "cangjie_lsp"),
                "No LSP tools should be registered without CANGJIE_HOME, but found: {:?}",
                tool_names
                    .iter()
                    .filter(|n| *n == "cangjie_lsp")
                    .collect::<Vec<_>>()
            );
        });
    }

    #[cfg(feature = "lsp")]
    #[test]
    fn test_get_info_with_cangjie_home() {
        temp_env::with_var("CANGJIE_HOME", Some("/some/path"), || {
            let settings = Settings {
                chunk_max_size: 6000,
                data_dir: std::path::PathBuf::from("/tmp/test-info"),
                openai_base_url: "https://api.example.com".to_string(),
                openai_model: "test".to_string(),
                ..Settings::default()
            };

            let server = CangjieServer::new(settings);
            let info = server.get_info();

            assert!(
                info.instructions.is_some(),
                "instructions should be present"
            );
            let instructions = info.instructions.unwrap();
            assert!(!instructions.is_empty(), "instructions should not be empty");

            let tools = server.tool_router.list_all();
            let tool_names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
            assert!(
                tool_names.iter().any(|n| n == "cangjie_lsp"),
                "LSP tools should be registered when CANGJIE_HOME is set"
            );
        });
    }

    #[tokio::test]
    async fn test_get_topic_not_initialized() {
        let settings = Settings {
            chunk_max_size: 6000,
            data_dir: std::path::PathBuf::from("/tmp/test-not-init"),
            openai_base_url: "https://api.example.com".to_string(),
            openai_model: "test".to_string(),
            ..Settings::default()
        };

        let server = CangjieServer::new(settings);
        let result = server
            .get_topic(rmcp::handler::server::wrapper::Parameters(
                super::GetTopicParams {
                    topic: "functions".to_string(),
                    category: None,
                },
            ))
            .await;

        assert_eq!(result, "Server not initialized");
    }

    #[tokio::test]
    async fn test_list_topics_not_initialized() {
        let settings = Settings {
            chunk_max_size: 6000,
            data_dir: std::path::PathBuf::from("/tmp/test-not-init"),
            openai_base_url: "https://api.example.com".to_string(),
            openai_model: "test".to_string(),
            ..Settings::default()
        };

        let server = CangjieServer::new(settings);
        let result = server
            .list_topics(rmcp::handler::server::wrapper::Parameters(
                super::ListTopicsParams {
                    category: None,
                    detail: false,
                },
            ))
            .await;

        assert_eq!(result, "Server not initialized");
    }
}
