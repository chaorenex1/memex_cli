use std::time::Duration;

use crate::state::types::RuntimePhase;
use crate::tool_event::ToolEvent;

#[derive(Debug, Clone)]
pub enum TuiEvent {
    ToolEvent(Box<ToolEvent>),
    AssistantOutput(String),
    RawStdout(String),
    RawStderr(String),
    StatusUpdate {
        tokens: u64,
        duration: Duration,
    },
    StateUpdate {
        phase: RuntimePhase,
        memory_hits: usize,
        tool_events: usize,
    },
    RunComplete {
        exit_code: i32,
    },
    Error(String),
}
