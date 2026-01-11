use memex_core::executor::traits::RetryStrategyPlugin;
use memex_core::executor::types::RetryConfig;
use std::time::Duration;

pub struct ExponentialBackoffPlugin {
    config: RetryConfig,
}

pub struct LinearRetryPlugin {
    config: RetryConfig,
}

impl ExponentialBackoffPlugin {
    pub fn new(config: RetryConfig) -> Self {
        Self { config }
    }
}

impl LinearRetryPlugin {
    pub fn new(config: RetryConfig) -> Self {
        Self { config }
    }
}

impl RetryStrategyPlugin for ExponentialBackoffPlugin {
    fn name(&self) -> &str {
        "exponential-backoff"
    }

    fn next_delay(&self, attempt: u32, _error: &str) -> Option<Duration> {
        if attempt >= self.config.max_attempts {
            return None;
        }
        let exp = 1u64 << attempt.min(30);
        let delay = self.config.base_delay_ms.saturating_mul(exp);
        let delay = delay.min(self.config.max_delay_ms);
        Some(Duration::from_millis(delay))
    }

    fn max_attempts(&self) -> u32 {
        self.config.max_attempts
    }
}

impl RetryStrategyPlugin for LinearRetryPlugin {
    fn name(&self) -> &str {
        "linear"
    }

    fn next_delay(&self, attempt: u32, _error: &str) -> Option<Duration> {
        if attempt >= self.config.max_attempts {
            return None;
        }
        let multiplier = attempt.saturating_add(1) as u64;
        let delay = self.config.base_delay_ms.saturating_mul(multiplier);
        let delay = delay.min(self.config.max_delay_ms);
        Some(Duration::from_millis(delay))
    }

    fn max_attempts(&self) -> u32 {
        self.config.max_attempts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exponential_backoff() {
        let cfg = RetryConfig {
            base_delay_ms: 100,
            max_delay_ms: 1000,
            max_attempts: 3,
            strategy: "exponential-backoff".to_string(),
        };
        let plugin = ExponentialBackoffPlugin::new(cfg);
        assert_eq!(plugin.next_delay(0, "err").unwrap().as_millis(), 100);
        assert_eq!(plugin.next_delay(1, "err").unwrap().as_millis(), 200);
        assert_eq!(plugin.next_delay(3, "err"), None);
    }

    #[test]
    fn test_linear_backoff() {
        let cfg = RetryConfig {
            base_delay_ms: 50,
            max_delay_ms: 200,
            max_attempts: 4,
            strategy: "linear".to_string(),
        };
        let plugin = LinearRetryPlugin::new(cfg);
        assert_eq!(plugin.next_delay(0, "err").unwrap().as_millis(), 50);
        assert_eq!(plugin.next_delay(2, "err").unwrap().as_millis(), 150);
    }
}
