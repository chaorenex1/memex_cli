use crate::tool_event::ToolEvent;

use std::collections::HashMap;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RunOutcome {
    pub exit_code: i32,
    pub duration_ms: Option<u64>,
    pub stdout_tail: String,
    pub stderr_tail: String,
    pub tool_events: Vec<ToolEvent>,

    pub shown_qa_ids: Vec<String>,
    pub used_qa_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub enum Signal {
    Kill,
    Term,
}

#[derive(Debug, Clone)]
pub enum PolicyAction {
    Allow,
    Deny { reason: String },
    Ask { prompt: String },
}

#[derive(Debug, Clone)]
pub struct RunnerStartArgs {
    pub cmd: String,
    pub args: Vec<String>,
    pub envs: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct RunnerResult {
    pub run_id: String,
    pub exit_code: i32,
    pub duration_ms: Option<u64>,
    pub stdout_tail: String,
    pub stderr_tail: String,
    pub tool_events: Vec<ToolEvent>,
    pub dropped_lines: u64,
}
