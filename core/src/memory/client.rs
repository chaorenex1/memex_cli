use serde_json::Value;

use super::models::{QACandidatePayload, QAHitsPayload, QASearchPayload, QAValidationPayload};

#[derive(Clone)]
pub struct MemoryClient {
    base_url: String,
    api_key: String,
    http: reqwest::Client,
}

impl MemoryClient {
    pub fn new(base_url: String, api_key: String, timeout_ms: u64) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(timeout_ms))
            .build()?;
        Ok(Self {
            base_url,
            api_key,
            http,
        })
    }

    fn auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if self.api_key.trim().is_empty() {
            req
        } else {
            req.bearer_auth(&self.api_key)
        }
    }

    pub async fn search(&self, payload: QASearchPayload) -> anyhow::Result<Value> {
        let url = format!("{}/v1/qa/search", self.base_url.trim_end_matches('/'));
        let req = self.http.post(url).json(&payload);
        let resp = self.auth(req).send().await?;
        let v = resp.json::<Value>().await?;
        Ok(v)
    }

    pub async fn send_hit(&self, payload: QAHitsPayload) -> anyhow::Result<Value> {
        let url = format!("{}/v1/qa/hit", self.base_url.trim_end_matches('/'));
        let req = self.http.post(url).json(&payload);
        Ok(self.auth(req).send().await?.json::<Value>().await?)
    }

    pub async fn send_candidate(&self, payload: QACandidatePayload) -> anyhow::Result<Value> {
        let url = format!("{}/v1/qa/candidates", self.base_url.trim_end_matches('/'));
        let req = self.http.post(url).json(&payload);
        Ok(self.auth(req).send().await?.json::<Value>().await?)
    }

    pub async fn send_validate(&self, payload: QAValidationPayload) -> anyhow::Result<Value> {
        let url: String = format!("{}/v1/qa/validate", self.base_url.trim_end_matches('/'));
        let req = self.http.post(url).json(&payload);
        Ok(self.auth(req).send().await?.json::<Value>().await?)
    }
}
