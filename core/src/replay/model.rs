use serde::Serialize;
use serde_json::Value;

use crate::tool_event::ToolEvent;
use crate::tool_event::WrapperEvent;

#[derive(Debug, Clone, Default, Serialize)]
pub struct ReplayRun {
    pub run_id: String,
    pub runner_start: Option<WrapperEvent>,
    pub runner_exit: Option<WrapperEvent>,
    pub tee_drop: Option<WrapperEvent>,
    pub memory_calls: Vec<WrapperEvent>,
    pub tool_events: Vec<ToolEvent>,
    pub search_result: Option<WrapperEvent>,
    pub gatekeeper_decision: Option<WrapperEvent>,
    pub derived: Value,
}
