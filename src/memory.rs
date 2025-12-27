use serde::{Deserialize, Serialize};
use serde_json::Value;

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
        Ok(Self { base_url, api_key, http })
    }

    fn auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if self.api_key.trim().is_empty() {
            req
        } else {
            // 你们服务端如果不是 Bearer，可改这里
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

    pub async fn hit(&self, payload: QAHitsPayload) -> anyhow::Result<Value> {
        let url = format!("{}/v1/qa/hit", self.base_url.trim_end_matches('/'));
        let req = self.http.post(url).json(&payload);
        Ok(self.auth(req).send().await?.json::<Value>().await?)
    }

    pub async fn candidate(&self, payload: QACandidatePayload) -> anyhow::Result<Value> {
        let url = format!("{}/v1/qa/candidates", self.base_url.trim_end_matches('/'));
        let req = self.http.post(url).json(&payload);
        Ok(self.auth(req).send().await?.json::<Value>().await?)
    }

    pub async fn validate(&self, payload: QAValidationPayload) -> anyhow::Result<Value> {
        let url = format!("{}/v1/qa/validate", self.base_url.trim_end_matches('/'));
        let req = self.http.post(url).json(&payload);
        Ok(self.auth(req).send().await?.json::<Value>().await?)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QASearchPayload {
    pub project_id: String,
    pub query: String,
    pub limit: u32,
    pub min_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QAReferencePayload {
    pub qa_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shown: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QAHitsPayload {
    pub project_id: String,
    pub references: Vec<QAReferencePayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QACandidatePayload {
    pub project_id: String,
    pub question: String,
    pub answer: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    #[serde(default)]
    pub confidence: f32,

    #[serde(default)]
    pub metadata: Value,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QAValidationPayload {
    pub project_id: String,
    pub qa_id: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal_strength: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strong_signal: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ts: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
}
