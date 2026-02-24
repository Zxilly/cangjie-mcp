use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::Result;
use jieba_rs::Jieba;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{schemars, tool, tool_handler, tool_router, ServerHandler};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::config::{
    Settings, DEFAULT_TOP_K, MAX_SUGGESTIONS, MAX_TOP_K, MIN_TOP_K, PACKAGE_FETCH_MULTIPLIER,
    SIMILARITY_THRESHOLD,
};
use crate::indexer::document::loader::extract_code_blocks;
use crate::indexer::document::source::{DocumentSource, GitDocumentSource, RemoteDocumentSource};
use crate::indexer::search::{LocalSearchIndex, RemoteSearchIndex};
use crate::indexer::SearchResult;
use crate::prompts::get_prompt;

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
pub struct TopicResult {
    pub content: String,
    pub file_path: String,
    pub category: String,
    pub topic: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TopicInfo {
    pub name: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TopicsListResult {
    pub categories: HashMap<String, Vec<TopicInfo>>,
    pub total_categories: usize,
    pub total_topics: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_categories: Option<Vec<String>>,
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
}

impl CangjieServer {
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

        match crate::lsp::detect_settings(Some(cwd.clone())) {
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

    pub fn new(settings: Settings) -> Self {
        Self {
            state: Arc::new(RwLock::new(None)),
            settings,
            tool_router: Self::tool_router(),
        }
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
            tool_router: Self::tool_router(),
        }
    }

    /// Initialize the server (clone repo, build index, etc.)
    pub async fn initialize(&self) -> Result<()> {
        let settings = self.settings.clone();
        info!("Initializing index...");
        Self::log_lsp_startup_status();
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        if let Some(lsp_settings) = crate::lsp::detect_settings(Some(cwd)) {
            if crate::lsp::init(lsp_settings).await {
                info!("LSP startup: initialization completed");
            } else {
                warn!("LSP startup: initialization failed, LSP tools will be unavailable");
            }
        } else {
            warn!("LSP startup: skipped initialization because CANGJIE_HOME is not configured");
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

        crate::config::log_startup_info(&settings, &index_info);

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
        let jieba = Jieba::new();
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

    fn lexical_boost(query_terms: &[String], query: &str, item: &SearchResult) -> f64 {
        let topic = item.metadata.topic.to_lowercase();
        let title = item.metadata.title.to_lowercase();
        let path = item.metadata.file_path.to_lowercase();
        let text = item.text.to_lowercase();
        let query_lc = query.to_lowercase();
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
            if topic.contains(&query_lc) {
                boost += 6.0;
            }
            if title.contains(&query_lc) {
                boost += 5.0;
            }
            if text.contains(&query_lc) {
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
        let query_terms = Self::query_terms(query);
        let mut scored: Vec<(SearchResult, f64)> = results
            .into_iter()
            .map(|r| {
                let adjusted = r.score + Self::lexical_boost(&query_terms, query, &r);
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

    /// Resolve a symbol name to 0-based (line, character) position via documentSymbol.
    async fn resolve_symbol(
        client: &crate::lsp::client::CangjieClient,
        file_path: &str,
        symbol: &str,
        line_hint: Option<u32>,
    ) -> Result<(u32, u32), String> {
        let result = client
            .document_symbol(file_path)
            .await
            .map_err(|e| format!("Failed to get symbols: {e}"))?;

        let syms = crate::lsp::tools::process_symbols(&result, file_path);

        // Collect all matching symbols (flatten hierarchy)
        let mut matches: Vec<(u32, u32)> = Vec::new();
        fn collect(
            symbols: &[crate::lsp::tools::SymbolOutput],
            name: &str,
            out: &mut Vec<(u32, u32)>,
        ) {
            for s in symbols {
                if s.name == name || s.name.starts_with(&format!("{name}(")) {
                    out.push((s.line, s.character));
                }
                if let Some(ref kids) = s.children {
                    collect(kids, name, out);
                }
            }
        }
        collect(&syms.symbols, symbol, &mut matches);

        if matches.is_empty() {
            let available: Vec<String> = syms.symbols.iter().map(|s| s.name.clone()).collect();
            return Err(format!(
                "Symbol '{}' not found in {}. Available: {:?}",
                symbol, file_path, available
            ));
        }

        let (line_1based, char_1based) = if matches.len() == 1 {
            matches[0]
        } else if let Some(hint) = line_hint {
            // Pick the match closest to the hint line
            *matches
                .iter()
                .min_by_key(|(l, _)| (*l as i64 - hint as i64).unsigned_abs())
                .unwrap()
        } else {
            return Err(format!(
                "Symbol '{}' appears {} times (lines: {:?}). Provide 'line' to disambiguate.",
                symbol,
                matches.len(),
                matches.iter().map(|(l, _)| *l).collect::<Vec<_>>()
            ));
        };

        // Convert to 0-based for LSP
        Ok((line_1based - 1, char_1based - 1))
    }

    fn lsp_unavailable_message() -> String {
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        if crate::lsp::detect_settings(Some(cwd)).is_none() {
            return "LSP is not available: CANGJIE_HOME is not configured. Set CANGJIE_HOME (and optionally CANGJIE_PATH) in environment variables.".to_string();
        }

        "LSP is not available: client is not initialized or failed to start. Check startup logs for 'LSP startup' and 'Failed to initialize LSP client'.".to_string()
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
    /// Whether to extract code examples from results
    #[serde(default)]
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
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LspSymbolParams {
    /// Absolute path to the .cj source file
    pub file_path: String,
    /// Symbol name to look up (e.g. "processArgs", "MyClass")
    pub symbol: String,
    /// Optional line number (1-based) to disambiguate when multiple symbols share the same name
    #[serde(default)]
    pub line: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LspFileParams {
    /// Absolute path to the .cj source file
    pub file_path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LspWorkspaceSymbolParams {
    /// Search query to find symbols by name across the workspace
    pub query: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LspRenameParams {
    /// Absolute path to the .cj source file
    pub file_path: String,
    /// Symbol name to rename
    pub symbol: String,
    /// New name for the symbol
    pub new_name: String,
    /// Optional line number (1-based) to disambiguate when multiple symbols share the same name
    #[serde(default)]
    pub line: Option<u32>,
}

// ── Tool implementations ────────────────────────────────────────────────────

#[tool_router]
impl CangjieServer {
    #[tool(
        name = "cangjie_search_docs",
        description = "Search Cangjie documentation using semantic search. Performs similarity search across all indexed documentation. Returns matching sections ranked by relevance with pagination support."
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
                    content: r.text,
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

        serde_json::to_string_pretty(&result)
            .unwrap_or_else(|e| format!("Serialization error: {e}"))
    }

    #[tool(
        name = "cangjie_get_topic",
        description = "Get complete documentation for a specific topic. Retrieves the full content of a documentation file by topic name. Use cangjie_list_topics first to discover available topic names."
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
            Ok(Some(doc)) => {
                let result = TopicResult {
                    content: doc.text,
                    file_path: doc.metadata.file_path,
                    category: doc.metadata.category,
                    topic: doc.metadata.topic,
                    title: doc.metadata.title,
                };
                serde_json::to_string_pretty(&result)
                    .unwrap_or_else(|e| format!("Serialization error: {e}"))
            }
            Ok(None) => {
                // If the provided category is wrong, fallback to cross-category search.
                if category.is_some() {
                    match docs.get_document_by_topic(topic, None).await {
                        Ok(Some(doc)) => {
                            let result = TopicResult {
                                content: doc.text,
                                file_path: doc.metadata.file_path,
                                category: doc.metadata.category,
                                topic: doc.metadata.topic,
                                title: doc.metadata.title,
                            };
                            return serde_json::to_string_pretty(&result)
                                .unwrap_or_else(|e| format!("Serialization error: {e}"));
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
        description = "List available documentation topics organized by category. Returns all documentation topics, optionally filtered by category. Use this to discover topic names for use with cangjie_get_topic."
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
                let result = TopicsListResult {
                    categories: HashMap::new(),
                    total_categories: 0,
                    total_topics: 0,
                    error: Some(format!("Category '{cat}' not found.")),
                    available_categories: Some(all_cats),
                };
                return serde_json::to_string_pretty(&result)
                    .unwrap_or_else(|e| format!("Serialization error: {e}"));
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
            let titles = docs.get_topic_titles(cat).await.unwrap_or_default();
            if !topics.is_empty() {
                let infos: Vec<TopicInfo> = topics
                    .iter()
                    .map(|t| TopicInfo {
                        name: t.clone(),
                        title: titles.get(t).cloned().unwrap_or_default(),
                    })
                    .collect();
                categories.insert(cat.clone(), infos);
            }
        }

        let total_topics: usize = categories.values().map(|v| v.len()).sum();
        let result = TopicsListResult {
            total_categories: categories.len(),
            total_topics,
            categories,
            error: None,
            available_categories: None,
        };

        serde_json::to_string_pretty(&result)
            .unwrap_or_else(|e| format!("Serialization error: {e}"))
    }

    // ── LSP Tools ──────────────────────────────────────────────────────────

    #[tool(
        name = "cangjie_lsp_definition",
        description = "Jump to the definition of a symbol in a .cj source file. Navigate to where a variable, function, class, etc. is defined."
    )]
    async fn lsp_definition(&self, Parameters(params): Parameters<LspSymbolParams>) -> String {
        use crate::lsp::tools as lsp_tools;

        if let Some(err) = lsp_tools::get_validate_error(&params.file_path) {
            return err;
        }

        let guard = crate::lsp::get_client().await;
        let client = match guard {
            Some(ref g) => match g.as_ref() {
                Some(c) => c,
                None => return Self::lsp_unavailable_message(),
            },
            None => return Self::lsp_unavailable_message(),
        };

        let (line, character) = match Self::resolve_symbol(
            client,
            &params.file_path,
            &params.symbol,
            params.line,
        )
        .await
        {
            Ok(pos) => pos,
            Err(e) => return e,
        };

        match client.definition(&params.file_path, line, character).await {
            Ok(result) => {
                let def = lsp_tools::process_definition(&result);
                serde_json::to_string_pretty(&def)
                    .unwrap_or_else(|e| format!("Serialization error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    #[tool(
        name = "cangjie_lsp_references",
        description = "Find all references to a symbol in a .cj source file. Locate all places where a symbol is used, including its definition."
    )]
    async fn lsp_references(&self, Parameters(params): Parameters<LspSymbolParams>) -> String {
        use crate::lsp::tools as lsp_tools;

        if let Some(err) = lsp_tools::get_validate_error(&params.file_path) {
            return err;
        }

        let guard = crate::lsp::get_client().await;
        let client = match guard {
            Some(ref g) => match g.as_ref() {
                Some(c) => c,
                None => return Self::lsp_unavailable_message(),
            },
            None => return Self::lsp_unavailable_message(),
        };

        let (line, character) = match Self::resolve_symbol(
            client,
            &params.file_path,
            &params.symbol,
            params.line,
        )
        .await
        {
            Ok(pos) => pos,
            Err(e) => return e,
        };

        match client.references(&params.file_path, line, character).await {
            Ok(result) => {
                let refs = lsp_tools::process_references(&result);
                serde_json::to_string_pretty(&refs)
                    .unwrap_or_else(|e| format!("Serialization error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    #[tool(
        name = "cangjie_lsp_hover",
        description = "Get hover information (type info and documentation) for a symbol in a .cj source file."
    )]
    async fn lsp_hover(&self, Parameters(params): Parameters<LspSymbolParams>) -> String {
        use crate::lsp::tools as lsp_tools;

        if let Some(err) = lsp_tools::get_validate_error(&params.file_path) {
            return err;
        }

        let guard = crate::lsp::get_client().await;
        let client = match guard {
            Some(ref g) => match g.as_ref() {
                Some(c) => c,
                None => return Self::lsp_unavailable_message(),
            },
            None => return Self::lsp_unavailable_message(),
        };

        let (line, character) = match Self::resolve_symbol(
            client,
            &params.file_path,
            &params.symbol,
            params.line,
        )
        .await
        {
            Ok(pos) => pos,
            Err(e) => return e,
        };

        match client.hover(&params.file_path, line, character).await {
            Ok(result) => lsp_tools::process_hover(&result, &params.file_path),
            Err(e) => format!("Error: {e}"),
        }
    }

    #[tool(
        name = "cangjie_lsp_symbols",
        description = "Get all symbols (classes, functions, variables, etc.) in a .cj source file. Returns a hierarchical list of symbols."
    )]
    async fn lsp_symbols(&self, Parameters(params): Parameters<LspFileParams>) -> String {
        use crate::lsp::tools as lsp_tools;

        if let Some(err) = lsp_tools::get_validate_error(&params.file_path) {
            return err;
        }

        let guard = crate::lsp::get_client().await;
        let client = match guard {
            Some(ref g) => match g.as_ref() {
                Some(c) => c,
                None => return Self::lsp_unavailable_message(),
            },
            None => return Self::lsp_unavailable_message(),
        };

        match client.document_symbol(&params.file_path).await {
            Ok(result) => {
                let syms = lsp_tools::process_symbols(&result, &params.file_path);
                serde_json::to_string_pretty(&syms)
                    .unwrap_or_else(|e| format!("Serialization error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    #[tool(
        name = "cangjie_lsp_diagnostics",
        description = "Get diagnostics (errors, warnings, hints) for a .cj source file. Retrieves compilation errors and warnings."
    )]
    async fn lsp_diagnostics(&self, Parameters(params): Parameters<LspFileParams>) -> String {
        use crate::lsp::tools as lsp_tools;

        if let Some(err) = lsp_tools::get_validate_error(&params.file_path) {
            return err;
        }

        let guard = crate::lsp::get_client().await;
        let client = match guard {
            Some(ref g) => match g.as_ref() {
                Some(c) => c,
                None => return Self::lsp_unavailable_message(),
            },
            None => return Self::lsp_unavailable_message(),
        };

        match client.get_diagnostics(&params.file_path).await {
            Ok(diags) => {
                let result = lsp_tools::process_diagnostics(&diags);
                serde_json::to_string_pretty(&result)
                    .unwrap_or_else(|e| format!("Serialization error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    #[tool(
        name = "cangjie_lsp_workspace_symbol",
        description = "Search for symbols (classes, functions, variables, etc.) by name across the entire workspace. Useful for finding where a type or function is defined without knowing the file."
    )]
    async fn lsp_workspace_symbol(
        &self,
        Parameters(params): Parameters<LspWorkspaceSymbolParams>,
    ) -> String {
        use crate::lsp::tools as lsp_tools;

        let guard = crate::lsp::get_client().await;
        let client = match guard {
            Some(ref g) => match g.as_ref() {
                Some(c) => c,
                None => return Self::lsp_unavailable_message(),
            },
            None => return Self::lsp_unavailable_message(),
        };

        match client.workspace_symbol(&params.query).await {
            Ok(result) => {
                let syms = lsp_tools::process_workspace_symbols(&result);
                serde_json::to_string_pretty(&syms)
                    .unwrap_or_else(|e| format!("Serialization error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    #[tool(
        name = "cangjie_lsp_incoming_calls",
        description = "Find all functions/methods that call the given function. Useful for understanding who uses a function."
    )]
    async fn lsp_incoming_calls(&self, Parameters(params): Parameters<LspSymbolParams>) -> String {
        use crate::lsp::tools as lsp_tools;

        if let Some(err) = lsp_tools::get_validate_error(&params.file_path) {
            return err;
        }

        let guard = crate::lsp::get_client().await;
        let client = match guard {
            Some(ref g) => match g.as_ref() {
                Some(c) => c,
                None => return Self::lsp_unavailable_message(),
            },
            None => return Self::lsp_unavailable_message(),
        };

        let (line, character) = match Self::resolve_symbol(
            client,
            &params.file_path,
            &params.symbol,
            params.line,
        )
        .await
        {
            Ok(pos) => pos,
            Err(e) => return e,
        };

        match client
            .incoming_calls(&params.file_path, line, character)
            .await
        {
            Ok(result) => {
                let calls = lsp_tools::process_incoming_calls(&result);
                serde_json::to_string_pretty(&calls)
                    .unwrap_or_else(|e| format!("Serialization error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    #[tool(
        name = "cangjie_lsp_outgoing_calls",
        description = "Find all functions/methods called by the given function. Useful for understanding what a function depends on."
    )]
    async fn lsp_outgoing_calls(&self, Parameters(params): Parameters<LspSymbolParams>) -> String {
        use crate::lsp::tools as lsp_tools;

        if let Some(err) = lsp_tools::get_validate_error(&params.file_path) {
            return err;
        }

        let guard = crate::lsp::get_client().await;
        let client = match guard {
            Some(ref g) => match g.as_ref() {
                Some(c) => c,
                None => return Self::lsp_unavailable_message(),
            },
            None => return Self::lsp_unavailable_message(),
        };

        let (line, character) = match Self::resolve_symbol(
            client,
            &params.file_path,
            &params.symbol,
            params.line,
        )
        .await
        {
            Ok(pos) => pos,
            Err(e) => return e,
        };

        match client
            .outgoing_calls(&params.file_path, line, character)
            .await
        {
            Ok(result) => {
                let calls = lsp_tools::process_outgoing_calls(&result);
                serde_json::to_string_pretty(&calls)
                    .unwrap_or_else(|e| format!("Serialization error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    #[tool(
        name = "cangjie_lsp_type_supertypes",
        description = "Find parent classes and implemented interfaces of the given type. Useful for understanding inheritance hierarchy."
    )]
    async fn lsp_type_supertypes(&self, Parameters(params): Parameters<LspSymbolParams>) -> String {
        use crate::lsp::tools as lsp_tools;

        if let Some(err) = lsp_tools::get_validate_error(&params.file_path) {
            return err;
        }

        let guard = crate::lsp::get_client().await;
        let client = match guard {
            Some(ref g) => match g.as_ref() {
                Some(c) => c,
                None => return Self::lsp_unavailable_message(),
            },
            None => return Self::lsp_unavailable_message(),
        };

        let (line, character) = match Self::resolve_symbol(
            client,
            &params.file_path,
            &params.symbol,
            params.line,
        )
        .await
        {
            Ok(pos) => pos,
            Err(e) => return e,
        };

        match client
            .type_supertypes(&params.file_path, line, character)
            .await
        {
            Ok(result) => {
                let types = lsp_tools::process_type_hierarchy(&result);
                serde_json::to_string_pretty(&types)
                    .unwrap_or_else(|e| format!("Serialization error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    #[tool(
        name = "cangjie_lsp_type_subtypes",
        description = "Find subclasses and implementations of the given type. Useful for finding all concrete implementations of an interface."
    )]
    async fn lsp_type_subtypes(&self, Parameters(params): Parameters<LspSymbolParams>) -> String {
        use crate::lsp::tools as lsp_tools;

        if let Some(err) = lsp_tools::get_validate_error(&params.file_path) {
            return err;
        }

        let guard = crate::lsp::get_client().await;
        let client = match guard {
            Some(ref g) => match g.as_ref() {
                Some(c) => c,
                None => return Self::lsp_unavailable_message(),
            },
            None => return Self::lsp_unavailable_message(),
        };

        let (line, character) = match Self::resolve_symbol(
            client,
            &params.file_path,
            &params.symbol,
            params.line,
        )
        .await
        {
            Ok(pos) => pos,
            Err(e) => return e,
        };

        match client
            .type_subtypes(&params.file_path, line, character)
            .await
        {
            Ok(result) => {
                let types = lsp_tools::process_type_hierarchy(&result);
                serde_json::to_string_pretty(&types)
                    .unwrap_or_else(|e| format!("Serialization error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    #[tool(
        name = "cangjie_lsp_rename",
        description = "Rename a symbol across the workspace. Returns the list of file edits needed (does not apply them)."
    )]
    async fn lsp_rename(&self, Parameters(params): Parameters<LspRenameParams>) -> String {
        use crate::lsp::tools as lsp_tools;

        if let Some(err) = lsp_tools::get_validate_error(&params.file_path) {
            return err;
        }

        let guard = crate::lsp::get_client().await;
        let client = match guard {
            Some(ref g) => match g.as_ref() {
                Some(c) => c,
                None => return Self::lsp_unavailable_message(),
            },
            None => return Self::lsp_unavailable_message(),
        };

        let (line, character) = match Self::resolve_symbol(
            client,
            &params.file_path,
            &params.symbol,
            params.line,
        )
        .await
        {
            Ok(pos) => pos,
            Err(e) => return e,
        };

        match client
            .rename(&params.file_path, line, character, &params.new_name)
            .await
        {
            Ok(result) => {
                let rename = lsp_tools::process_rename(&result);
                serde_json::to_string_pretty(&rename)
                    .unwrap_or_else(|e| format!("Serialization error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }
}

// ── ServerHandler impl ──────────────────────────────────────────────────────

#[tool_handler]
impl ServerHandler for CangjieServer {
    fn get_info(&self) -> ServerInfo {
        let lsp_enabled = crate::lsp::is_available();
        ServerInfo {
            instructions: Some(get_prompt(lsp_enabled)),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::{SearchResult, SearchResultMetadata};

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
        });
    }

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
                super::ListTopicsParams { category: None },
            ))
            .await;

        assert_eq!(result, "Server not initialized");
    }
}
