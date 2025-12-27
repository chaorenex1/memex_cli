use memex_core::config::AppConfig;
use memex_core::stream::{StreamPlan, StreamStrategy};

pub struct JsonlStreamStrategy;

impl StreamStrategy for JsonlStreamStrategy {
    fn apply(&self, cfg: &mut AppConfig) -> StreamPlan {
        // For JSONL mode we force wrapper/tool events to stdout and suppress raw stdout/stderr.
        cfg.events_out.enabled = true;
        cfg.events_out.path = "stdout:".to_string();

        StreamPlan { silent: true }
    }
}
