pub mod config;
pub mod decision;
pub mod evaluate;
pub mod gatekeeper_reasons;
mod helpers;
pub mod signals;
pub mod r#trait;

pub use config::GatekeeperConfig;
pub use decision::{GatekeeperDecision, InjectItem, SearchMatch, TaskGradeResult};
pub use evaluate::Gatekeeper;
pub use helpers::{
    extract_final_answer_from_tool_events, extract_final_reasoning_from_tool_events,
    extract_qa_refs_from_tool_events,
};
pub use r#trait::GatekeeperPlugin;
