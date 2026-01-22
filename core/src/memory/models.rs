use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    pub signal_strength: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strong_signal: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ts: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
}
