use memex_core::config::AppConfig;
use memex_core::stream::{StreamPlan, StreamStrategy};

pub struct TextStreamStrategy;

impl StreamStrategy for TextStreamStrategy {
    fn apply(&self, _cfg: &mut AppConfig) -> StreamPlan {
        StreamPlan { silent: false }
    }
}
