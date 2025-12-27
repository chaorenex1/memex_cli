use crate::config::AppConfig;

#[derive(Debug, Clone, Copy)]
pub struct StreamPlan {
    /// If true, suppress raw stdout/stderr forwarding in the tee.
    /// This is typically used when the process output is expected to be clean JSONL.
    pub silent: bool,
}

pub trait StreamStrategy: Send + Sync {
    /// Apply any stream/output-related config overrides and return a plan for runtime behavior.
    fn apply(&self, cfg: &mut AppConfig) -> StreamPlan;
}
