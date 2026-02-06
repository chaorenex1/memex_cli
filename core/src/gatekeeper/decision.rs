use serde::{Deserialize, Serialize};
use serde_json::Value;
/**
 {
  "task_level": "L0 | L1 | L2 | L3",
  "reason": "<one-sentence justification>",
  "recommended_model": "<model_name>",
  "confidence": 0.0-1.0
}
 */
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskGradeResult {
    pub task_level: String,
    pub reason: String,
    pub recommended_model: String,
    pub recommended_model_provider: Option<String>,
    pub confidence: f32,
}

/// Search match from memory service - all fields have defaults for robust deserialization
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchMatch {
    #[serde(default)]
    pub qa_id: String,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub question: String,
    #[serde(default)]
    pub answer: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub score: f32,
    #[serde(default)]
    pub relevance: f32,
    #[serde(default)]
    pub validation_level: i32,
    #[serde(default)]
    pub level: Option<String>,
    #[serde(default)]
    pub trust: f32,
    #[serde(default)]
    pub freshness: f32,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub expiry_at: Option<String>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjectItem {
    pub qa_id: String,
    pub question: String,
    pub answer: String,
    pub summary: Option<String>,
    pub trust: f32,
    pub validation_level: i32,
    pub score: f32,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HitRef {
    pub qa_id: String,
    pub shown: bool,
    pub used: bool,
    pub message_id: Option<String>,
    pub context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatePlan {
    pub qa_id: String,
    pub result: String,
    pub signal_strength: String,
    pub strong_signal: bool,
    pub context: Option<Value>,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatekeeperDecision {
    pub inject_list: Vec<InjectItem>,
    pub should_write_candidate: bool,

    pub hit_refs: Vec<HitRef>,

    pub validate_plans: Vec<ValidatePlan>,

    pub reasons: Vec<String>,

    pub signals: Value,

    pub candidate_drafts: Vec<crate::memory::CandidateDraft>,
}
