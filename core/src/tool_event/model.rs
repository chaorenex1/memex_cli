use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const TOOL_EVENT_PREFIX: &str = "@@MEM_TOOL_EVENT@@";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolEvent {
    pub v: i32,

    #[serde(rename = "type")]
    pub event_type: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub ts: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,

    #[serde(default)]
    pub args: Value,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
}

impl Default for ToolEvent {
    fn default() -> Self {
        Self {
            v: 1,
            event_type: String::new(),
            ts: None,
            run_id: None,
            id: None,
            tool: None,
            action: None,
            args: Value::Null,
            ok: None,
            output: None,
            error: None,
            rationale: None,
        }
    }
}
