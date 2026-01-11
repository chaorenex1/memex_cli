use memex_core::executor::traits::{ConcurrencyContext, ConcurrencyStrategyPlugin};
use memex_core::executor::types::ConcurrencyConfig;

pub struct AdaptiveConcurrencyPlugin {
    config: ConcurrencyConfig,
}

pub struct FixedConcurrencyPlugin {
    fixed: usize,
}

impl AdaptiveConcurrencyPlugin {
    pub fn new(config: ConcurrencyConfig) -> Self {
        Self { config }
    }
}

impl FixedConcurrencyPlugin {
    pub fn new(fixed: usize) -> Self {
        Self { fixed }
    }
}

impl ConcurrencyStrategyPlugin for AdaptiveConcurrencyPlugin {
    fn name(&self) -> &str {
        "adaptive"
    }

    fn calculate_concurrency(&self, context: &ConcurrencyContext) -> usize {
        let mut desired = context.base_concurrency;

        if context.cpu_usage >= self.config.cpu_threshold_high {
            desired = desired.saturating_div(2).max(self.config.min_concurrency);
        } else if context.cpu_usage <= self.config.cpu_threshold_low {
            desired = desired.saturating_mul(2).min(self.config.max_concurrency);
        }

        desired = desired.clamp(self.config.min_concurrency, self.config.max_concurrency);
        desired.clamp(1, context.available_cpus.max(1))
    }
}

impl ConcurrencyStrategyPlugin for FixedConcurrencyPlugin {
    fn name(&self) -> &str {
        "fixed"
    }

    fn calculate_concurrency(&self, _context: &ConcurrencyContext) -> usize {
        self.fixed.max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(cpu: f32, base: usize) -> ConcurrencyContext {
        ConcurrencyContext {
            cpu_usage: cpu,
            available_cpus: 8,
            memory_usage: 0.0,
            active_tasks: 0,
            base_concurrency: base,
        }
    }

    #[test]
    fn test_adaptive_concurrency() {
        let cfg = ConcurrencyConfig {
            strategy: "adaptive".to_string(),
            min_concurrency: 2,
            max_concurrency: 8,
            base_concurrency: 4,
            cpu_threshold_low: 30.0,
            cpu_threshold_high: 80.0,
        };
        let plugin = AdaptiveConcurrencyPlugin::new(cfg);

        assert_eq!(plugin.calculate_concurrency(&context(10.0, 4)), 8);
        assert_eq!(plugin.calculate_concurrency(&context(90.0, 4)), 2);
        assert_eq!(plugin.calculate_concurrency(&context(50.0, 4)), 4);
    }

    #[test]
    fn test_fixed_concurrency() {
        let plugin = FixedConcurrencyPlugin::new(3);
        assert_eq!(plugin.calculate_concurrency(&context(0.0, 1)), 3);
    }
}
