//! HTTP API数据模型

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

// ============= Search =============

#[derive(Debug, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    pub project_id: String,
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default = "default_min_score")]
    pub min_score: f32,
}

fn default_limit() -> u32 {
    5
}

fn default_min_score() -> f32 {
    0.6
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

// ============= Record Candidate =============

#[derive(Debug, Deserialize)]
pub struct RecordCandidateRequest {
    pub project_id: String,
    pub question: String,
    pub answer: String,
}

#[derive(Debug, Serialize)]
pub struct RecordCandidateResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

// ============= Record Hit =============

#[derive(Debug, Deserialize)]
pub struct RecordHitRequest {
    pub project_id: String,
    pub qa_ids: Vec<String>,
    #[serde(default)]
    pub shown_ids: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct RecordHitResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

// ============= Validate =============

#[derive(Debug, Deserialize)]
pub struct ValidateRequest {
    pub project_id: String,
    pub qa_id: String,
    pub result: String, // "success" | "fail"
    #[serde(default)]
    pub signal_strength: Option<String>, // "strong" | "weak"
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct ValidateResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

// ============= Record Validation =============

#[derive(Debug, Deserialize)]
pub struct RecordValidationRequest {
    pub project_id: String,
    pub qa_id: String,
    pub success: bool,
    #[serde(default = "default_confidence")]
    pub confidence: f32,
}

fn default_confidence() -> f32 {
    0.8
}

#[derive(Debug, Serialize)]
pub struct RecordValidationResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

// ============= Health =============

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub session_id: String,
    pub uptime_seconds: f64,
    pub requests_handled: u64,
    pub timestamp: String,
}

// ============= Error Handling =============

#[derive(Debug)]
pub enum HttpServerError {
    InvalidRequest(String),
    MemoryService(String),
    Timeout,
    Internal(String),
}

impl IntoResponse for HttpServerError {
    fn into_response(self) -> Response {
        let (status, error_code, message) = match self {
            Self::InvalidRequest(msg) => (StatusCode::BAD_REQUEST, "INVALID_REQUEST", msg),
            Self::MemoryService(msg) => (StatusCode::BAD_GATEWAY, "MEMORY_SERVICE_ERROR", msg),
            Self::Timeout => (
                StatusCode::GATEWAY_TIMEOUT,
                "TIMEOUT",
                "Request timeout".to_string(),
            ),
            Self::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", msg),
        };

        let body = serde_json::json!({
            "success": false,
            "error": message,
            "error_code": error_code,
        });

        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn test_search_request_deserialize() {
        let json = r#"{"query":"test","project_id":"proj1","limit":10,"min_score":0.7}"#;
        let req: SearchRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.query, "test");
        assert_eq!(req.project_id, "proj1");
        assert_eq!(req.limit, 10);
        assert_eq!(req.min_score, 0.7);
    }

    #[test]
    fn test_search_request_defaults() {
        let json = r#"{"query":"test","project_id":"proj1"}"#;
        let req: SearchRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.limit, 5);
        assert_eq!(req.min_score, 0.6);
    }

    #[test]
    fn test_record_candidate_request_deserialize() {
        let json = r#"{"project_id":"proj1","question":"Q","answer":"A"}"#;
        let req: RecordCandidateRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.project_id, "proj1");
        assert_eq!(req.question, "Q");
        assert_eq!(req.answer, "A");
    }

    #[test]
    fn test_record_validation_request_defaults() {
        let json = r#"{"project_id":"proj1","qa_id":"qa1","success":true}"#;
        let req: RecordValidationRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.project_id, "proj1");
        assert_eq!(req.qa_id, "qa1");
        assert!(req.success);
        assert_eq!(req.confidence, 0.8);
    }

    #[test]
    fn test_search_response_serialize() {
        let resp = SearchResponse {
            success: true,
            data: Some(serde_json::json!({"count": 5})),
            error: None,
            error_code: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"count\":5"));
        assert!(!json.contains("\"error\""));
    }

    #[test]
    fn test_evaluate_session_request_deserialize_structured_output() {
        let json = r#"{
            "project_id":"proj1",
            "user_query":"q",
            "tool_events":[
                {"tool":"shell","args":{"command":"echo hi"},"output":[{"type":"text","text":"hi"}],"code":0},
                {"tool":"shell","args":{},"output":{"type":"text","text":"ok"},"code":0},
                {"tool":"shell","args":{},"output":"plain","code":0}
            ],
            "stdout":"",
            "stderr":"",
            "shown_qa_ids":[],
            "used_qa_ids":[],
            "exit_code":0,
            "duration_ms":10
        }"#;

        let req: EvaluateSessionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.tool_events.len(), 3);
        assert!(matches!(req.tool_events[0].output, Some(Value::Array(_))));
        assert!(matches!(req.tool_events[1].output, Some(Value::Object(_))));
        assert!(matches!(req.tool_events[2].output, Some(Value::String(_))));
    }
}

// ============= Evaluate Session =============

/// Tool event from transcript (simplified for HTTP API)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolEventSimple {
    pub tool: String,
    #[serde(default)]
    pub args: serde_json::Value,
    #[serde(default)]
    pub output: Option<serde_json::Value>,
    #[serde(default)]
    pub code: Option<i32>,
}

/// Evaluate session request with parsed transcript data
#[derive(Debug, Deserialize)]
pub struct EvaluateSessionRequest {
    pub project_id: String,
    pub user_query: String,
    pub tool_events: Vec<ToolEventSimple>,
    pub stdout: String,
    pub stderr: String,
    pub shown_qa_ids: Vec<String>,
    pub used_qa_ids: Vec<String>,
    pub exit_code: i32,
    #[serde(default)]
    pub duration_ms: u64,
}

/// Evaluate session response with gatekeeper decision
#[derive(Debug, Serialize)]
pub struct EvaluateSessionResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_summary: Option<String>,
    pub candidates_recorded: usize,
    pub hits_recorded: usize,
    pub validations_recorded: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}
