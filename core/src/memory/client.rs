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
        tracing::debug!(
            target: "memex.qa",
            stage = "memory.http.search.in",
            url = %url,
            project_id = %payload.project_id,
            query_len = payload.query.len(),
            limit = payload.limit,
            min_score = payload.min_score
        );
        let req = self.http.post(url).json(&payload);
        let resp = self.auth(req).send().await?;
        let status = resp.status();
        let v = resp.json::<Value>().await?;
        tracing::debug!(
            target: "memex.qa",
            stage = "memory.http.search.out",
            status = %status
        );
        Ok(v)
    }

    pub async fn send_hit(&self, payload: QAHitsPayload) -> anyhow::Result<Value> {
        let url = format!("{}/v1/qa/hit", self.base_url.trim_end_matches('/'));
        let used = payload
            .references
            .iter()
            .filter(|r| r.used == Some(true))
            .count();
        let shown = payload
            .references
            .iter()
            .filter(|r| r.shown == Some(true))
            .count();
        tracing::debug!(
            target: "memex.qa",
            stage = "memory.http.hit.in",
            url = %url,
            project_id = %payload.project_id,
            references = payload.references.len(),
            shown = shown,
            used = used
        );
        let req = self.http.post(url).json(&payload);
        let resp = self.auth(req).send().await?;
        let status = resp.status();
        let v = resp.json::<Value>().await?;
        tracing::debug!(target: "memex.qa", stage = "memory.http.hit.out", status = %status);
        Ok(v)
    }

    pub async fn send_candidate(&self, payload: QACandidatePayload) -> anyhow::Result<Value> {
        let url = format!("{}/v1/qa/candidates", self.base_url.trim_end_matches('/'));
        tracing::debug!(
            target: "memex.qa",
            stage = "memory.http.candidate.in",
            url = %url,
            project_id = %payload.project_id,
            tags = payload.tags.len()
        );
        let req = self.http.post(url).json(&payload);
        let resp = self.auth(req).send().await?;
        let status = resp.status();
        let v = resp.json::<Value>().await?;
        tracing::debug!(
            target: "memex.qa",
            stage = "memory.http.candidate.out",
            status = %status
        );
        Ok(v)
    }

    pub async fn send_validate(&self, payload: QAValidationPayload) -> anyhow::Result<Value> {
        let url: String = format!("{}/v1/qa/validate", self.base_url.trim_end_matches('/'));
        tracing::debug!(
            target: "memex.qa",
            stage = "memory.http.validate.in",
            url = %url,
            project_id = %payload.project_id,
            qa_id = %payload.qa_id,
            result = ?payload.result
        );
        let req = self.http.post(url).json(&payload);
        let resp = self.auth(req).send().await?;
        let status = resp.status();
        let v = resp.json::<Value>().await?;
        tracing::debug!(
            target: "memex.qa",
            stage = "memory.http.validate.out",
            status = %status
        );
        Ok(v)
    }

    pub async  fn task_grade(&self, prompt: String) -> anyhow::Result<Value> {
        let url = format!("{}/v1/task/grade", self.base_url.trim_end_matches('/'));
        tracing::debug!(
            target: "memex.task",
            stage = "memory.http.task_grade.in",
            url = %url
        );
        let req = self
            .http
            .post(url)
            .json(&serde_json::json!({ "prompt": prompt }));
        let resp = self.auth(req).send().await?;
        let status = resp.status();
        let v = resp.json::<Value>().await?;
        tracing::debug!(
            target: "memex.task",
            stage = "memory.http.task_grade.out",
            status = %status
        );
        Ok(v)
    }
}
