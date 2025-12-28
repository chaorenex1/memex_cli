pub mod config;
pub mod decision;
pub mod evaluate;
pub mod gatekeeper_reasons;
mod helpers;
pub mod signals;
pub mod r#trait;

pub use config::GatekeeperConfig;
pub use decision::{GatekeeperDecision, HitRef, InjectItem, SearchMatch, ValidatePlan};
pub use evaluate::Gatekeeper;
pub use helpers::extract_qa_refs;
pub use r#trait::GatekeeperPlugin;
pub use signals::{build_signals, grade_validation_signal, SignalHeuristics, ValidationSignal};
