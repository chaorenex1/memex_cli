use anyhow::Result;

use memex_core::backend::BackendStrategy;
use memex_core::config::{
    AppConfig, GatekeeperProvider, MemoryProvider, PolicyProvider, RunnerConfig,
};
use memex_core::runner::RunnerPlugin;
use memex_core::stream::StreamStrategy;

use crate::backend::{AiServiceBackendStrategy, CodeCliBackendStrategy};
use crate::gatekeeper::StandardGatekeeperPlugin;
use crate::memory::service::MemoryServicePlugin;
use crate::policy::config_rules::ConfigPolicyPlugin;
use crate::runner::codecli::CodeCliRunnerPlugin;
use crate::runner::replay::ReplayRunnerPlugin;
use crate::stream::{JsonlStreamStrategy, TextStreamStrategy};

pub fn build_memory(cfg: &AppConfig) -> Result<Option<Box<dyn memex_core::memory::MemoryPlugin>>> {
    if !cfg.memory.enabled {
        return Ok(None);
    }

    match &cfg.memory.provider {
        MemoryProvider::Service(svc_cfg) => Ok(Some(Box::new(MemoryServicePlugin::new(
            svc_cfg.base_url.clone(),
            svc_cfg.api_key.clone(),
            svc_cfg.timeout_ms,
        )?))),
    }
}

pub fn build_runner(cfg: &AppConfig) -> Box<dyn RunnerPlugin> {
    match &cfg.runner {
        RunnerConfig::CodeCli(_) => Box::new(CodeCliRunnerPlugin::new()),
        RunnerConfig::Replay(r_cfg) => Box::new(ReplayRunnerPlugin::new(r_cfg.events_file.clone())),
    }
}

pub fn build_policy(cfg: &AppConfig) -> Option<Box<dyn memex_core::runner::PolicyPlugin>> {
    match &cfg.policy.provider {
        PolicyProvider::Config(_) => Some(Box::new(ConfigPolicyPlugin::new(cfg.policy.clone()))),
    }
}

pub fn build_gatekeeper(cfg: &AppConfig) -> Box<dyn memex_core::gatekeeper::GatekeeperPlugin> {
    match &cfg.gatekeeper.provider {
        GatekeeperProvider::Standard(std_cfg) => {
            Box::new(StandardGatekeeperPlugin::new(std_cfg.clone().into()))
        }
    }
}

pub fn build_stream(stream_format: &str) -> Box<dyn StreamStrategy> {
    match stream_format {
        "jsonl" => Box::new(JsonlStreamStrategy),
        // Preserve existing behavior: anything other than jsonl behaves like text.
        _ => Box::new(TextStreamStrategy),
    }
}

pub fn build_backend(backend: &str) -> Box<dyn BackendStrategy> {
    if backend.starts_with("http://") || backend.starts_with("https://") {
        Box::new(AiServiceBackendStrategy)
    } else {
        Box::new(CodeCliBackendStrategy)
    }
}

pub fn build_backend_with_kind(kind: &str, backend: &str) -> Box<dyn BackendStrategy> {
    match kind {
        "aiservice" => Box::new(AiServiceBackendStrategy),
        "codecli" => Box::new(CodeCliBackendStrategy),
        // Preserve existing behavior.
        _ => build_backend(backend),
    }
}
