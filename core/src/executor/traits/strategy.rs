use std::time::Duration;

/// 重试策略插件
pub trait RetryStrategyPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn next_delay(&self, attempt: u32, error: &str) -> Option<Duration>;
    fn max_attempts(&self) -> u32;
    fn should_retry(&self, attempt: u32, error: &str) -> bool {
        attempt < self.max_attempts() && !self.is_fatal_error(error)
    }
    fn is_fatal_error(&self, _error: &str) -> bool {
        false
    }
}

/// 并发控制策略插件
pub trait ConcurrencyStrategyPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn calculate_concurrency(&self, context: &ConcurrencyContext) -> usize;
}

#[derive(Debug, Clone)]
pub struct ConcurrencyContext {
    pub cpu_usage: f32,
    pub available_cpus: usize,
    pub memory_usage: f32,
    pub active_tasks: usize,
    pub base_concurrency: usize,
}
