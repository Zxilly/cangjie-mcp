use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{schemars, tool, tool_handler, tool_router, ServerHandler};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::info;

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

        let (search, index_info) = if let Some(ref url) = settings.server_url {
            let remote = RemoteSearchIndex::new(url)?;
            let info = remote.init().await?;
            (SearchBackend::Remote(Arc::new(remote)), info)
        } else {
            let mut local = LocalSearchIndex::new(settings.clone()).await;
            let info = local.init().await?;
            (SearchBackend::Local(Arc::new(local)), info)
        };

        crate::config::log_startup_info(&settings, &index_info);

        let docs: Arc<dyn DocumentSource> = if let Some(ref url) = settings.server_url {
            Arc::new(RemoteDocumentSource::new(url))
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
pub struct LspPositionParams {
    /// Absolute path to the .cj source file
    pub file_path: String,
    /// Line number (1-based)
    pub line: u32,
    /// Character position (1-based)
    pub character: u32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LspFileParams {
    /// Absolute path to the .cj source file
    pub file_path: String,
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

        let fetch_multiplier = if package.is_some() {
            PACKAGE_FETCH_MULTIPLIER
        } else {
            1
        };
        let fetch_count = (params.offset + top_k + 1) * fetch_multiplier;

        let mut results = match self.do_search(&params.query, fetch_count, category).await {
            Ok(r) => r,
            Err(e) => return format!("Search error: {e}"),
        };

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
                if !suggestions.is_empty() {
                    let names: Vec<&str> = suggestions.iter().map(|(n, _)| n.as_str()).collect();
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
        description = "Jump to the definition of a symbol in a .cj source file. Navigate to where a variable, function, class, etc. is defined. Positions are 1-based."
    )]
    async fn lsp_definition(&self, Parameters(params): Parameters<LspPositionParams>) -> String {
        use crate::lsp::tools as lsp_tools;

        if let Some(err) = lsp_tools::get_validate_error(&params.file_path) {
            return err;
        }

        let guard = crate::lsp::get_client().await;
        let client = match guard {
            Some(ref g) => match g.as_ref() {
                Some(c) => c,
                None => return "LSP is not available. Ensure CANGJIE_HOME is set.".to_string(),
            },
            None => return "LSP is not available. Ensure CANGJIE_HOME is set.".to_string(),
        };

        match client
            .definition(&params.file_path, params.line - 1, params.character - 1)
            .await
        {
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
        description = "Find all references to a symbol in a .cj source file. Locate all places where a symbol is used, including its definition. Positions are 1-based."
    )]
    async fn lsp_references(&self, Parameters(params): Parameters<LspPositionParams>) -> String {
        use crate::lsp::tools as lsp_tools;

        if let Some(err) = lsp_tools::get_validate_error(&params.file_path) {
            return err;
        }

        let guard = crate::lsp::get_client().await;
        let client = match guard {
            Some(ref g) => match g.as_ref() {
                Some(c) => c,
                None => return "LSP is not available. Ensure CANGJIE_HOME is set.".to_string(),
            },
            None => return "LSP is not available. Ensure CANGJIE_HOME is set.".to_string(),
        };

        match client
            .references(&params.file_path, params.line - 1, params.character - 1)
            .await
        {
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
        description = "Get hover information (type info and documentation) for a symbol in a .cj source file. Positions are 1-based."
    )]
    async fn lsp_hover(&self, Parameters(params): Parameters<LspPositionParams>) -> String {
        use crate::lsp::tools as lsp_tools;

        if let Some(err) = lsp_tools::get_validate_error(&params.file_path) {
            return err;
        }

        let guard = crate::lsp::get_client().await;
        let client = match guard {
            Some(ref g) => match g.as_ref() {
                Some(c) => c,
                None => return "LSP is not available. Ensure CANGJIE_HOME is set.".to_string(),
            },
            None => return "LSP is not available. Ensure CANGJIE_HOME is set.".to_string(),
        };

        match client
            .hover(&params.file_path, params.line - 1, params.character - 1)
            .await
        {
            Ok(result) => lsp_tools::process_hover(&result, &params.file_path),
            Err(e) => format!("Error: {e}"),
        }
    }

    #[tool(
        name = "cangjie_lsp_completion",
        description = "Get code completion suggestions at a position in a .cj source file. Positions are 1-based."
    )]
    async fn lsp_completion(&self, Parameters(params): Parameters<LspPositionParams>) -> String {
        use crate::lsp::tools as lsp_tools;

        if let Some(err) = lsp_tools::get_validate_error(&params.file_path) {
            return err;
        }

        let guard = crate::lsp::get_client().await;
        let client = match guard {
            Some(ref g) => match g.as_ref() {
                Some(c) => c,
                None => return "LSP is not available. Ensure CANGJIE_HOME is set.".to_string(),
            },
            None => return "LSP is not available. Ensure CANGJIE_HOME is set.".to_string(),
        };

        match client
            .completion(&params.file_path, params.line - 1, params.character - 1)
            .await
        {
            Ok(result) => {
                let comp = lsp_tools::process_completion(&result);
                serde_json::to_string_pretty(&comp)
                    .unwrap_or_else(|e| format!("Serialization error: {e}"))
            }
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
                None => return "LSP is not available. Ensure CANGJIE_HOME is set.".to_string(),
            },
            None => return "LSP is not available. Ensure CANGJIE_HOME is set.".to_string(),
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
                None => return "LSP is not available. Ensure CANGJIE_HOME is set.".to_string(),
            },
            None => return "LSP is not available. Ensure CANGJIE_HOME is set.".to_string(),
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
}

// ── ServerHandler impl ──────────────────────────────────────────────────────

#[tool_handler]
impl ServerHandler for CangjieServer {
    fn get_info(&self) -> ServerInfo {
        let lsp_enabled = std::env::var("CANGJIE_HOME").is_ok();
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
        std::env::remove_var("CANGJIE_HOME");

        let settings = Settings {
            docs_version: "dev".to_string(),
            docs_lang: crate::config::DocLang::Zh,
            embedding_type: crate::config::EmbeddingType::None,
            local_model: String::new(),
            rerank_type: crate::config::RerankType::None,
            rerank_model: String::new(),
            rerank_top_k: 5,
            rerank_initial_k: 20,
            rrf_k: 60,
            chunk_max_size: 6000,
            data_dir: std::path::PathBuf::from("/tmp/test-info"),
            server_url: None,
            openai_api_key: None,
            openai_base_url: "https://api.example.com".to_string(),
            openai_model: "test".to_string(),
            prebuilt: crate::config::PrebuiltMode::Off,
        };

        let server = CangjieServer::new(settings);
        let info = server.get_info();

        assert!(
            info.instructions.is_some(),
            "instructions should be present"
        );
        let instructions = info.instructions.unwrap();
        assert!(!instructions.is_empty(), "instructions should not be empty");
    }

    #[test]
    fn test_get_info_with_cangjie_home() {
        std::env::set_var("CANGJIE_HOME", "/some/path");

        let settings = Settings {
            docs_version: "dev".to_string(),
            docs_lang: crate::config::DocLang::Zh,
            embedding_type: crate::config::EmbeddingType::None,
            local_model: String::new(),
            rerank_type: crate::config::RerankType::None,
            rerank_model: String::new(),
            rerank_top_k: 5,
            rerank_initial_k: 20,
            rrf_k: 60,
            chunk_max_size: 6000,
            data_dir: std::path::PathBuf::from("/tmp/test-info"),
            server_url: None,
            openai_api_key: None,
            openai_base_url: "https://api.example.com".to_string(),
            openai_model: "test".to_string(),
            prebuilt: crate::config::PrebuiltMode::Off,
        };

        let server = CangjieServer::new(settings);
        let info = server.get_info();

        assert!(
            info.instructions.is_some(),
            "instructions should be present"
        );
        let instructions = info.instructions.unwrap();
        assert!(!instructions.is_empty(), "instructions should not be empty");
        std::env::remove_var("CANGJIE_HOME");
    }

    #[tokio::test]
    async fn test_get_topic_not_initialized() {
        let settings = Settings {
            docs_version: "dev".to_string(),
            docs_lang: crate::config::DocLang::Zh,
            embedding_type: crate::config::EmbeddingType::None,
            local_model: String::new(),
            rerank_type: crate::config::RerankType::None,
            rerank_model: String::new(),
            rerank_top_k: 5,
            rerank_initial_k: 20,
            rrf_k: 60,
            chunk_max_size: 6000,
            data_dir: std::path::PathBuf::from("/tmp/test-not-init"),
            server_url: None,
            openai_api_key: None,
            openai_base_url: "https://api.example.com".to_string(),
            openai_model: "test".to_string(),
            prebuilt: crate::config::PrebuiltMode::Off,
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
            docs_version: "dev".to_string(),
            docs_lang: crate::config::DocLang::Zh,
            embedding_type: crate::config::EmbeddingType::None,
            local_model: String::new(),
            rerank_type: crate::config::RerankType::None,
            rerank_model: String::new(),
            rerank_top_k: 5,
            rerank_initial_k: 20,
            rrf_k: 60,
            chunk_max_size: 6000,
            data_dir: std::path::PathBuf::from("/tmp/test-not-init"),
            server_url: None,
            openai_api_key: None,
            openai_base_url: "https://api.example.com".to_string(),
            openai_model: "test".to_string(),
            prebuilt: crate::config::PrebuiltMode::Off,
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
