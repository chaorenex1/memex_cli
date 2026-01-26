//! ServicesFactory 实现：从配置构建并统一提供 policy/memory/gatekeeper 等 services，供 CLI 复用。
use async_trait::async_trait;
use memex_core::api::{AppConfig, RunnerError, Services, ServicesFactory};

use crate::factory;

pub struct PluginServicesFactory;

impl Default for PluginServicesFactory {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl ServicesFactory for PluginServicesFactory {
    async fn build_services(&self, cfg: &AppConfig) -> Result<Services, RunnerError> {
        let memory = factory::build_memory(cfg)
            .await
            .map_err(RunnerError::Plugin)?;
        let policy = factory::build_policy(cfg);
        let gatekeeper = factory::build_gatekeeper(cfg);
        Ok(Services {
            policy,
            memory,
            gatekeeper,
        })
    }
}
