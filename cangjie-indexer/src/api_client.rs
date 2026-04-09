//! Layered HTTP clients: `HttpClient` (base) and `ApiClient` (with Bearer auth).

use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::StatusCode;
use serde::{de::DeserializeOwned, Serialize};
use tracing::warn;

use cangjie_core::config::Settings;

const DEFAULT_POST_JSON_MAX_RETRIES: u32 = 5;
const DEFAULT_RETRY_INITIAL_BACKOFF_SECS: u64 = 2;
const DEFAULT_RETRY_MAX_BACKOFF_SECS: u64 = 30;

/// Build a shared HTTP client optimized for external API calls.
fn build_http_client(settings: &Settings, timeout: Duration) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(
            cangjie_core::config::DEFAULT_HTTP_CONNECT_TIMEOUT_SECS,
        ))
        .pool_idle_timeout(Duration::from_secs(settings.http_pool_idle_timeout_secs))
        .pool_max_idle_per_host(settings.http_pool_max_idle_per_host)
        .tcp_keepalive(Duration::from_secs(settings.http_tcp_keepalive_secs));

    if settings.http_enable_http2 {
        builder = builder.http2_adaptive_window(true);
    }

    builder.build().context("Failed to build HTTP client")
}

fn response_body_excerpt(body: &str) -> String {
    const MAX_CHARS: usize = 600;

    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "<empty body>".to_string();
    }

    let mut excerpt: String = trimmed.chars().take(MAX_CHARS).collect();
    if trimmed.chars().count() > MAX_CHARS {
        excerpt.push_str("... [truncated]");
    }
    excerpt
}

fn response_headers_excerpt(headers: &reqwest::header::HeaderMap) -> String {
    let mut lines = headers
        .iter()
        .map(|(name, value)| {
            let value = value.to_str().unwrap_or("<non-utf8>");
            format!("{name}: {value}")
        })
        .collect::<Vec<_>>();
    lines.sort();

    if lines.is_empty() {
        "<no headers>".to_string()
    } else {
        lines.join("; ")
    }
}

async fn decode_json_response<T: DeserializeOwned>(
    response: reqwest::Response,
    request_label: &str,
) -> Result<T> {
    let status = response.status();
    let url = response.url().clone();
    let headers = response_headers_excerpt(response.headers());
    let body = response.text().await.with_context(|| {
        format!("Failed to read {request_label} response body from {url} (HTTP {status})")
    })?;
    let excerpt = response_body_excerpt(&body);

    if !status.is_success() {
        anyhow::bail!(
            "{request_label} failed: HTTP {status} from {url}; headers: {headers}; body: {excerpt}"
        );
    }

    serde_json::from_str(&body).with_context(|| {
        format!(
            "Invalid {request_label} response from {url} (HTTP {status}); headers: {headers}; body: {excerpt}"
        )
    })
}

fn is_retryable_status(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::TOO_MANY_REQUESTS
            | StatusCode::SERVICE_UNAVAILABLE
            | StatusCode::GATEWAY_TIMEOUT
    )
}

fn retry_backoff_delay(retry_index: u32) -> Duration {
    let shift = retry_index.saturating_sub(1).min(31);
    let multiplier = 1_u64.checked_shl(shift).unwrap_or(u64::MAX);
    let secs = DEFAULT_RETRY_INITIAL_BACKOFF_SECS
        .saturating_mul(multiplier)
        .min(DEFAULT_RETRY_MAX_BACKOFF_SECS);
    Duration::from_secs(secs)
}

// -- HttpClient (no auth) ----------------------------------------------------

/// A generic HTTP client with base-URL handling and optional retry support.
#[derive(Clone)]
pub(crate) struct HttpClient {
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

    /// Create a POST request to `{base_url}/{endpoint}`.
    pub fn post(&self, endpoint: &str) -> reqwest::RequestBuilder {
        self.client.post(self.url_for(endpoint))
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    async fn send_json_with_retry<T, F>(
        &self,
        request_label: &str,
        max_retries: u32,
        mut build_request: F,
    ) -> Result<T>
    where
        T: DeserializeOwned,
        F: FnMut() -> reqwest::RequestBuilder,
    {
        for attempt in 0..=max_retries {
            match build_request().send().await {
                Ok(response) => {
                    let status = response.status();
                    if attempt < max_retries && is_retryable_status(status) {
                        let delay = retry_backoff_delay(attempt + 1);
                        warn!(
                            "{request_label} returned HTTP {} on attempt {}/{}; retrying in {:?}",
                            status,
                            attempt + 1,
                            max_retries + 1,
                            delay
                        );
                        tokio::time::sleep(delay).await;
                        continue;
                    }

                    return decode_json_response(response, request_label).await;
                }
                Err(err) => {
                    if attempt < max_retries {
                        let delay = retry_backoff_delay(attempt + 1);
                        warn!(
                            "{request_label} attempt {}/{} failed: {}; retrying in {:?}",
                            attempt + 1,
                            max_retries + 1,
                            err,
                            delay
                        );
                        tokio::time::sleep(delay).await;
                        continue;
                    }

                    anyhow::bail!(
                        "Failed to {request_label} after {} attempts: {err}",
                        max_retries + 1
                    );
                }
            }
        }

        unreachable!("retry loop always returns or continues")
    }

    pub async fn post_json<P: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        endpoint: &str,
        payload: &P,
    ) -> Result<T> {
        let request_label = format!("POST /{endpoint}");
        self.send_json_with_retry(&request_label, DEFAULT_POST_JSON_MAX_RETRIES, || {
            self.post(endpoint).json(payload)
        })
        .await
    }

    /// GET with retry + exponential backoff, for init/startup scenarios.
    pub async fn get_with_retry<T: DeserializeOwned>(
        &self,
        endpoint: &str,
        max_retries: u32,
    ) -> Result<T> {
        let url = self.url_for(endpoint);
        let request_label = format!("GET /{endpoint}");
        self.send_json_with_retry(&request_label, max_retries, || self.client.get(&url))
            .await
    }
}

// -- ApiClient (Bearer auth) -------------------------------------------------

/// An authenticated HTTP client for OpenAI-compatible API endpoints.
#[derive(Clone)]
pub(crate) struct ApiClient {
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

    pub async fn post_json<P: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        endpoint: &str,
        payload: &P,
    ) -> Result<T> {
        let request_label = format!("POST /{endpoint}");
        self.http
            .send_json_with_retry(&request_label, DEFAULT_POST_JSON_MAX_RETRIES, || {
                self.post(endpoint).json(payload)
            })
            .await
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn base_url(&self) -> &str {
        self.http.base_url()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cangjie_core::config::Settings;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    struct MockResponse {
        status_line: &'static str,
        headers: &'static [(&'static str, &'static str)],
        body: &'static str,
    }

    async fn spawn_json_server(responses: Vec<MockResponse>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            for response in responses {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut buf = [0_u8; 4096];
                let _ = stream.read(&mut buf).await.unwrap();

                let headers = response
                    .headers
                    .iter()
                    .map(|(k, v)| format!("{k}: {v}\r\n"))
                    .collect::<String>();

                let payload = format!(
                    "HTTP/1.1 {}\r\nContent-Type: application/json\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response.status_line,
                    response.body.len(),
                    response.body,
                );
                stream.write_all(payload.as_bytes()).await.unwrap();
            }
        });

        format!("http://{addr}")
    }

    #[tokio::test]
    async fn post_json_includes_status_and_error_body_on_http_failure() {
        let base_url = spawn_json_server(vec![MockResponse {
            status_line: "400 Bad Request",
            headers: &[("X-SiliconCloud-Trace-Id", "trace-123")],
            body: r#"{"code":400,"message":"bad request","data":null}"#,
        }])
        .await;
        let client = ApiClient::new(
            &Settings::default(),
            "test-key",
            "test-model",
            &base_url,
            Duration::from_secs(5),
        )
        .unwrap();

        let err = client
            .post_json::<_, serde_json::Value>(
                "embeddings",
                &serde_json::json!({"model":"test-model","input":["hello"]}),
            )
            .await
            .unwrap_err();

        let msg = format!("{err:#}");
        assert!(msg.contains("HTTP 400 Bad Request"), "{msg}");
        assert!(msg.contains("/embeddings"), "{msg}");
        assert!(msg.contains("bad request"), "{msg}");
        assert!(msg.contains("x-siliconcloud-trace-id: trace-123"), "{msg}");
    }

    #[tokio::test]
    async fn post_json_retries_rate_limited_response_without_retry_after() {
        let base_url = spawn_json_server(vec![
            MockResponse {
                status_line: "429 Too Many Requests",
                headers: &[],
                body: r#"{"code":429,"message":"slow down","data":null}"#,
            },
            MockResponse {
                status_line: "200 OK",
                headers: &[],
                body: r#"{"ok":true}"#,
            },
        ])
        .await;
        let client = ApiClient::new(
            &Settings::default(),
            "test-key",
            "test-model",
            &base_url,
            Duration::from_secs(5),
        )
        .unwrap();

        let body = client
            .post_json::<_, serde_json::Value>(
                "embeddings",
                &serde_json::json!({"model":"test-model","input":["hello"]}),
            )
            .await
            .unwrap();

        assert_eq!(body, serde_json::json!({"ok": true}));
    }
}
