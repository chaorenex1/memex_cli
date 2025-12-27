use crate::config::AppConfig;

use super::types::StreamPlan;

pub trait StreamStrategy: Send + Sync {
    /// Apply any stream/output-related config overrides and return a plan for runtime behavior.
    fn apply(&self, cfg: &mut AppConfig) -> StreamPlan;
}
