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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMatch {
    pub qa_id: String,
    pub project_id: Option<String>,
    pub question: String,
    pub answer: String,
    pub tags: Vec<String>,
    pub score: f32,
    pub relevance: f32, // Similar to score?
    pub validation_level: i32,
    pub level: Option<String>,
    pub trust: f32,
    pub freshness: f32,
    pub confidence: f32,
    pub status: String,
    pub summary: Option<String>,
    pub source: Option<String>,
    pub expiry_at: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
pub struct HitRef {
    pub qa_id: String,
    pub shown: bool,
    pub used: bool,
    pub message_id: Option<String>,
    pub context: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidatePlan {
    pub qa_id: String,
    pub result: String,
    pub signal_strength: String,
    pub strong_signal: bool,
    pub context: Option<Value>,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct GatekeeperDecision {
    pub inject_list: Vec<InjectItem>,
    pub should_write_candidate: bool,

    pub hit_refs: Vec<HitRef>,
    pub validate_plans: Vec<ValidatePlan>,

    pub reasons: Vec<String>,
    pub signals: Value,
}
