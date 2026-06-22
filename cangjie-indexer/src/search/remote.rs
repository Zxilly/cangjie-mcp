use anyhow::Result;
use tracing::info;

use crate::api_client::HttpClient;
use crate::SearchResult;
use crate::SearchResultMetadata;
use cangjie_core::config::{DocLang, IndexInfo, Settings};

#[derive(Debug, serde::Deserialize)]
struct RemoteInfoResponse {
    #[serde(default)]
    version: String,
    #[serde(default)]
    lang: String,
    #[serde(default)]
    embedding_model: String,
}

#[derive(Debug, serde::Serialize)]
struct RemoteSearchRequest {
    query: String,
    top_k: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct RemoteSearchResponse {
    #[serde(default)]
    results: Vec<RemoteSearchResultItem>,
}

#[derive(Debug, serde::Deserialize)]
struct RemoteSearchResultItem {
    #[serde(default)]
    text: String,
    #[serde(default)]
    score: f64,
    #[serde(default)]
    metadata: SearchResultMetadata,
}

pub struct RemoteSearchIndex {
    http: HttpClient,
}

impl RemoteSearchIndex {
    pub fn new(settings: &Settings, server_url: &str) -> Result<Self> {
        Ok(Self {
            http: HttpClient::new(settings, server_url, std::time::Duration::from_secs(60))?,
        })
    }

    pub fn base_url(&self) -> &str {
        self.http.base_url()
    }

    pub async fn init(&self) -> Result<IndexInfo> {
        info!("Connecting to remote server: {}", self.http.base_url());

        let data: RemoteInfoResponse = self.http.get_with_retry("info", 3).await?;
        let lang = match data.lang.as_str() {
            "en" => DocLang::En,
            _ => DocLang::Zh,
        };
        Ok(IndexInfo {
            version: data.version,
            lang,
            embedding_model_name: data.embedding_model,
            data_dir: cangjie_core::config::get_default_data_dir(),
        })
    }

    pub async fn query(
        &self,
        query: &str,
        top_k: usize,
        category: Option<&str>,
    ) -> Result<Vec<SearchResult>> {
        let payload = RemoteSearchRequest {
            query: query.to_string(),
            top_k,
            category: category.map(|s| s.to_string()),
        };

        let data: RemoteSearchResponse = self.http.post_json("search", &payload).await?;

        Ok(data
            .results
            .into_iter()
            .map(|item| SearchResult {
                text: item.text,
                score: item.score,
                metadata: item.metadata,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::test_settings;
    use std::path::PathBuf;

    #[test]
    fn test_remote_search_new_trailing_slash() {
        let remote = RemoteSearchIndex::new(
            &test_settings(PathBuf::from("/tmp")),
            "http://localhost:8765/",
        )
        .unwrap();
        assert_eq!(
            remote.base_url(),
            "http://localhost:8765",
            "Trailing slash should be trimmed"
        );
    }

    #[test]
    fn test_remote_search_new_multiple_trailing_slashes() {
        let remote = RemoteSearchIndex::new(
            &test_settings(PathBuf::from("/tmp")),
            "http://example.com///",
        )
        .unwrap();
        assert_eq!(
            remote.base_url(),
            "http://example.com",
            "All trailing slashes should be trimmed"
        );
    }
}
