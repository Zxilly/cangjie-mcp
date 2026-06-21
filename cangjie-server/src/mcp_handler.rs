use std::sync::Arc;

#[cfg(feature = "lsp")]
use crate::lsp_pool::LspPool;
use anyhow::Result;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{tool, tool_handler, tool_router, ServerHandler};
use tokio::sync::RwLock;
use tracing::info;
#[cfg(feature = "lsp")]
use tracing::warn;

use cangjie_core::config::{Settings, MAX_TOP_K, MIN_TOP_K, PACKAGE_FETCH_MULTIPLIER};
use cangjie_core::prompts::get_prompt;
use cangjie_indexer::document::chunker::strip_chunk_artifacts;
use cangjie_indexer::search::{LocalSearchIndex, RemoteSearchIndex};
use cangjie_indexer::SearchResult;

mod ranking;
mod results;

pub use results::{DocsSearchResult, SearchDocsParams, SearchResultItem};

use results::format_results_markdown;

#[derive(Clone)]
enum SearchBackend {
    Local(Arc<LocalSearchIndex>),
    Remote(Arc<RemoteSearchIndex>),
}

struct InnerState {
    search: SearchBackend,
}

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
        ToolRouter::<Self>::new().with_route((Self::search_docs_tool_attr(), Self::search_docs))
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

    /// Create a server with an LSP pool for daemon mode (clients created on demand per workspace).
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

    /// Create a `CangjieServer` with pre-initialized shared state.
    pub fn with_shared_state(settings: Settings, search: Arc<LocalSearchIndex>) -> Self {
        let inner = InnerState {
            search: SearchBackend::Local(search),
        };
        Self {
            state: Arc::new(RwLock::new(Some(inner))),
            settings,
            tool_router: Self::build_tool_router(),
            #[cfg(feature = "lsp")]
            lsp_pool: None,
        }
    }

    /// Create a `CangjieServer` with pre-initialized state (for testing).
    #[doc(hidden)]
    pub fn with_local_state(settings: Settings, search: LocalSearchIndex) -> Self {
        Self::with_shared_state(settings, Arc::new(search))
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

        let inner = InnerState { search };
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
            SearchBackend::Local(local) => local.query(query, top_k, category).await,
            SearchBackend::Remote(remote) => remote.query(query, top_k, category).await,
        }
    }
}

#[tool_router]
impl CangjieServer {
    #[tool(
        name = "cangjie_lsp",
        description = "Unified Cangjie LSP entry point. Use operation to run definition, references, hover, document_symbol, diagnostics, workspace_symbol, incoming_calls, outgoing_calls, and type hierarchy.",
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

        // Fetch extra candidates so reranking, dedup, and pagination have headroom.
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
            .map(|r| SearchResultItem {
                content: strip_chunk_artifacts(&r.text).to_string(),
                score: r.score,
                file_path: r.metadata.file_path,
                category: r.metadata.category,
                topic: r.metadata.topic,
                title: r.metadata.title,
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
}

// rmcp 1.7's `#[tool_handler]` defaults to the static `Self::tool_router()`, which
// would expose every tool unconditionally. Point it at the instance field so the
// conditional router from `build_tool_router()` (LSP tool only when
// `cangjie_lsp::is_available()`) is what's actually served.
#[tool_handler(router = self.tool_router)]
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

    #[test]
    fn test_get_info_without_cangjie_home() {
        temp_env::with_var("CANGJIE_HOME", None::<&str>, || {
            let settings = Settings {
                max_chunk_chars: Some(6000),
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
                max_chunk_chars: Some(6000),
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
}
