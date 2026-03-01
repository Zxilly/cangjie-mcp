//! Shared API client for OpenAI-compatible endpoints (embedding, reranking, chat).

#[derive(Clone)]
pub struct ApiClient {
    model: String,
    base_url: String,
    auth_header: String,
    client: reqwest::Client,
}

impl ApiClient {
    pub fn new(client: reqwest::Client, api_key: &str, model: &str, base_url: &str) -> Self {
        Self {
            auth_header: format!("Bearer {}", api_key),
            model: model.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
            client,
        }
    }

    /// Create a POST request to `{base_url}/{endpoint}` with Bearer auth.
    pub fn post(&self, endpoint: &str) -> reqwest::RequestBuilder {
        let url = format!("{}/{}", self.base_url, endpoint);
        self.client
            .post(&url)
            .header("Authorization", &self.auth_header)
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}
