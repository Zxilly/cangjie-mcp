//! Layered HTTP clients: `HttpClient` (base) and `ApiClient` (with Bearer auth).

use std::time::Duration;

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use tracing::warn;

use crate::config::Settings;

/// Build a shared HTTP client optimized for external API calls.
fn build_http_client(settings: &Settings, timeout: Duration) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(
            crate::config::DEFAULT_HTTP_CONNECT_TIMEOUT_SECS,
        ))
        .pool_idle_timeout(Duration::from_secs(settings.http_pool_idle_timeout_secs))
        .pool_max_idle_per_host(settings.http_pool_max_idle_per_host)
        .tcp_keepalive(Duration::from_secs(settings.http_tcp_keepalive_secs));

    if settings.http_enable_http2 {
        builder = builder.http2_adaptive_window(true);
    }

    builder.build().context("Failed to build HTTP client")
}

// -- HttpClient (no auth) ----------------------------------------------------

/// A generic HTTP client with base-URL handling and optional retry support.
#[derive(Clone)]
pub struct HttpClient {
    base_url: String,
    client: reqwest::Client,
}

impl HttpClient {
    pub fn new(settings: &Settings, base_url: &str, timeout: Duration) -> Result<Self> {
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: build_http_client(settings, timeout)?,
        })
    }

    fn url_for(&self, endpoint: &str) -> String {
        format!("{}/{}", self.base_url, endpoint)
    }

    /// Create a GET request to `{base_url}/{endpoint}`.
    pub fn get(&self, endpoint: &str) -> reqwest::RequestBuilder {
        self.client.get(self.url_for(endpoint))
    }

    /// Create a POST request to `{base_url}/{endpoint}`.
    pub fn post(&self, endpoint: &str) -> reqwest::RequestBuilder {
        self.client.post(self.url_for(endpoint))
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// GET with retry + exponential backoff, for init/startup scenarios.
    pub async fn get_with_retry<T: DeserializeOwned>(
        &self,
        endpoint: &str,
        max_retries: u32,
    ) -> Result<T> {
        const RETRY_BACKOFF_SECS: u64 = 2;
        let url = self.url_for(endpoint);

        let mut last_err = None;
        for attempt in 1..=max_retries {
            match self.client.get(&url).send().await {
                Ok(resp) => {
                    let data: T = resp
                        .json()
                        .await
                        .with_context(|| format!("Invalid /{endpoint} response"))?;
                    return Ok(data);
                }
                Err(e) => {
                    warn!(
                        "Request to /{} attempt {}/{} failed: {}",
                        endpoint, attempt, max_retries, e
                    );
                    last_err = Some(e);
                    if attempt < max_retries {
                        tokio::time::sleep(Duration::from_secs(
                            RETRY_BACKOFF_SECS * attempt as u64,
                        ))
                        .await;
                    }
                }
            }
        }

        Err(last_err
            .map(|e| anyhow::anyhow!(e))
            .unwrap_or_else(|| anyhow::anyhow!("Request failed"))
            .context(format!(
                "Failed to GET /{} after {} retries",
                endpoint, max_retries
            )))
    }
}

// -- ApiClient (Bearer auth) -------------------------------------------------

/// An authenticated HTTP client for OpenAI-compatible API endpoints.
#[derive(Clone)]
pub struct ApiClient {
    http: HttpClient,
    model: String,
    auth_header: String,
}

impl ApiClient {
    pub fn new(
        settings: &Settings,
        api_key: &str,
        model: &str,
        base_url: &str,
        timeout: Duration,
    ) -> Result<Self> {
        Ok(Self {
            http: HttpClient::new(settings, base_url, timeout)?,
            auth_header: format!("Bearer {}", api_key),
            model: model.to_string(),
        })
    }

    /// Create a POST request to `{base_url}/{endpoint}` with Bearer auth.
    pub fn post(&self, endpoint: &str) -> reqwest::RequestBuilder {
        self.http
            .post(endpoint)
            .header("Authorization", &self.auth_header)
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn base_url(&self) -> &str {
        self.http.base_url()
    }
}
