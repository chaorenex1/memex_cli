use anyhow::Result;
use std::sync::Arc;

use memex_core::api as core_api;

use crate::backend::{AiServiceBackendStrategy, CodeCliBackendStrategy};
use crate::gatekeeper::StandardGatekeeperPlugin;
use crate::memory::service::MemoryServicePlugin;
use crate::policy::config_rules::ConfigPolicyPlugin;
use crate::runner::codecli::CodeCliRunnerPlugin;
use crate::runner::replay::ReplayRunnerPlugin;

pub fn build_memory(cfg: &core_api::AppConfig) -> Result<Option<Arc<dyn core_api::MemoryPlugin>>> {
    if !cfg.memory.enabled {
        return Ok(None);
    }

    match &cfg.memory.provider {
        core_api::MemoryProvider::Service(svc_cfg) => Ok(Some(Arc::new(MemoryServicePlugin::new(
            svc_cfg.base_url.clone(),
            svc_cfg.api_key.clone(),
            svc_cfg.timeout_ms,
        )?))),
    }
}

pub fn build_runner(cfg: &core_api::AppConfig) -> Box<dyn core_api::RunnerPlugin> {
    match &cfg.runner {
        core_api::RunnerConfig::CodeCli(_) => Box::new(CodeCliRunnerPlugin::new()),
        core_api::RunnerConfig::Replay(r_cfg) => {
            Box::new(ReplayRunnerPlugin::new(r_cfg.events_file.clone()))
        }
    }
}

pub fn build_policy(cfg: &core_api::AppConfig) -> Option<Arc<dyn core_api::PolicyPlugin>> {
    match &cfg.policy.provider {
        core_api::PolicyProvider::Config(_) => {
            Some(Arc::new(ConfigPolicyPlugin::new(cfg.policy.clone())))
        }
    }
}

pub fn build_gatekeeper(cfg: &core_api::AppConfig) -> Arc<dyn core_api::GatekeeperPlugin> {
    match &cfg.gatekeeper.provider {
        core_api::GatekeeperProvider::Standard(std_cfg) => {
            Arc::new(StandardGatekeeperPlugin::new(std_cfg.clone().into()))
        }
    }
}

pub fn build_backend(backend: &str) -> Box<dyn core_api::BackendStrategy> {
    if backend.starts_with("http://") || backend.starts_with("https://") {
        Box::new(AiServiceBackendStrategy)
    } else {
        Box::new(CodeCliBackendStrategy)
    }
}

pub fn build_backend_with_kind(kind: &str, backend: &str) -> Box<dyn core_api::BackendStrategy> {
    match kind {
        "aiservice" => Box::new(AiServiceBackendStrategy),
        "codecli" => Box::new(CodeCliBackendStrategy),
        // Preserve existing behavior.
        _ => build_backend(backend),
    }
}
