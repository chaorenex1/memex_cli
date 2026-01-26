use anyhow::Result;
use std::sync::Arc;

use memex_core::api as core_api;
use memex_core::executor::traits::{
    ConcurrencyStrategyPlugin, OutputRendererPlugin, RetryStrategyPlugin, TaskProcessorPlugin,
};

use crate::backend::{AiServiceBackendStrategy, CodeCliBackendStrategy};
use crate::executor::{
    AdaptiveConcurrencyPlugin, ContextInjectorPlugin, ExponentialBackoffPlugin,
    FileProcessorPlugin, FixedConcurrencyPlugin, JsonlRendererPlugin, LinearRetryPlugin,
    PromptEnhancerPlugin, TextRendererPlugin,
};
use crate::gatekeeper::StandardGatekeeperPlugin;
use crate::memory::hybrid::{HybridMemoryConfig, HybridMemoryPlugin};
use crate::memory::local::{EmbeddingConfig, LocalMemoryConfig, LocalMemoryPlugin};
use crate::memory::service::MemoryServicePlugin;
use crate::memory::sync::SyncConfig;
use crate::policy::config_rules::ConfigPolicyPlugin;
use crate::runner::codecli::CodeCliRunnerPlugin;
use crate::runner::replay::ReplayRunnerPlugin;

pub async fn build_memory(
    cfg: &core_api::AppConfig,
) -> Result<Option<Arc<dyn core_api::MemoryPlugin>>> {
    if !cfg.memory.enabled {
        return Ok(None);
    }

    match &cfg.memory.provider {
        core_api::MemoryProvider::Service(svc_cfg) => Ok(Some(Arc::new(MemoryServicePlugin::new(
            svc_cfg.base_url.clone(),
            svc_cfg.api_key.clone(),
            svc_cfg.timeout_ms,
        )?))),
        core_api::MemoryProvider::Local(local_cfg) => {
            // Build embedding config
            let embedding = match &local_cfg.embedding.provider {
                core_api::EmbeddingProvider::Ollama => {
                    let ollama = local_cfg.embedding.ollama.as_ref().ok_or_else(|| {
                        anyhow::anyhow!("Ollama configuration is required for provider=ollama")
                    })?;
                    EmbeddingConfig::Ollama {
                        base_url: ollama.base_url.clone(),
                        model: ollama.model.clone(),
                        dimension: ollama.dimension,
                    }
                }
                core_api::EmbeddingProvider::OpenAI => {
                    let openai = local_cfg.embedding.openai.as_ref().ok_or_else(|| {
                        anyhow::anyhow!("OpenAI configuration is required for provider=openai")
                    })?;
                    EmbeddingConfig::OpenAI {
                        base_url: openai.base_url.clone(),
                        api_key: openai.api_key.clone(),
                        model: openai.model.clone(),
                    }
                }
                core_api::EmbeddingProvider::Local => {
                    return Err(anyhow::anyhow!(
                        "Local embedding provider is not supported. Please use Ollama or OpenAI."
                    ))
                }
            };

            // Expand home directory in db_path
            let db_path = shellexpand::tilde(&local_cfg.db_path).to_string();

            let plugin = LocalMemoryPlugin::new(LocalMemoryConfig {
                db_path,
                embedding,
                search_limit: local_cfg.search_limit,
                min_score: local_cfg.min_score,
            })
            .await?;

            Ok(Some(Arc::new(plugin)))
        }
        core_api::MemoryProvider::Hybrid(hybrid_cfg) => {
            // Build embedding config from local config
            let embedding = match &hybrid_cfg.local.embedding.provider {
                core_api::EmbeddingProvider::Ollama => {
                    let ollama = hybrid_cfg.local.embedding.ollama.as_ref().ok_or_else(|| {
                        anyhow::anyhow!("Ollama configuration is required for provider=ollama")
                    })?;
                    EmbeddingConfig::Ollama {
                        base_url: ollama.base_url.clone(),
                        model: ollama.model.clone(),
                        dimension: ollama.dimension,
                    }
                }
                core_api::EmbeddingProvider::OpenAI => {
                    let openai = hybrid_cfg.local.embedding.openai.as_ref().ok_or_else(|| {
                        anyhow::anyhow!("OpenAI configuration is required for provider=openai")
                    })?;
                    EmbeddingConfig::OpenAI {
                        base_url: openai.base_url.clone(),
                        api_key: openai.api_key.clone(),
                        model: openai.model.clone(),
                    }
                }
                core_api::EmbeddingProvider::Local => {
                    return Err(anyhow::anyhow!(
                        "Local embedding provider is not supported. Please use Ollama or OpenAI."
                    ))
                }
            };

            // Expand home directory
            let db_path = shellexpand::tilde(&hybrid_cfg.local.db_path).to_string();

            // Build sync config - use core_api types directly
            let sync_config = SyncConfig {
                enabled: hybrid_cfg.local.sync.enabled,
                interval: std::time::Duration::from_secs(hybrid_cfg.local.sync.interval_secs),
                batch_size: hybrid_cfg.local.sync.batch_size,
                max_retries: hybrid_cfg.local.sync.max_retries,
                retry_delay_ms: 1000,
                conflict_resolution: hybrid_cfg.local.sync.conflict_resolution,
            };

            let local_config = LocalMemoryConfig {
                db_path,
                embedding,
                search_limit: hybrid_cfg.local.search_limit,
                min_score: hybrid_cfg.local.min_score,
            };

            let hybrid_config = HybridMemoryConfig {
                local: local_config,
                remote_base_url: hybrid_cfg.remote.base_url.clone(),
                remote_api_key: hybrid_cfg.remote.api_key.clone(),
                remote_timeout_ms: hybrid_cfg.remote.timeout_ms,
                sync_strategy: hybrid_cfg.sync_strategy,
                sync: sync_config,
            };

            let plugin = HybridMemoryPlugin::new(hybrid_config).await?;

            Ok(Some(Arc::new(plugin)))
        }
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

/// 构建任务处理器插件链
pub fn build_task_processors(cfg: &core_api::ExecutionConfig) -> Vec<Arc<dyn TaskProcessorPlugin>> {
    let mut processors: Vec<Arc<dyn TaskProcessorPlugin>> = Vec::new();

    if cfg.file_processing.enabled {
        let file_processor = FileProcessorPlugin::new(cfg.file_processing.clone());
        processors.push(Arc::new(file_processor));
    }

    processors.push(Arc::new(ContextInjectorPlugin::new()));
    processors.push(Arc::new(PromptEnhancerPlugin::new()));

    processors.sort_by_key(|p| std::cmp::Reverse(p.priority()));
    processors
}

/// 构建输出渲染器插件
pub fn build_renderer(format: &str, cfg: &core_api::OutputConfig) -> Arc<dyn OutputRendererPlugin> {
    let format = if format.is_empty() {
        cfg.format.as_str()
    } else {
        format
    };

    match format {
        "jsonl" => Arc::new(JsonlRendererPlugin::new(cfg.pretty_print)),
        "text" => Arc::new(TextRendererPlugin::new(cfg.ascii_only)),
        _ => Arc::new(TextRendererPlugin::new(cfg.ascii_only)),
    }
}

/// 构建重试策略插件
pub fn build_retry_strategy(cfg: &core_api::RetryConfig) -> Arc<dyn RetryStrategyPlugin> {
    match cfg.strategy.as_str() {
        "linear" => Arc::new(LinearRetryPlugin::new(cfg.clone())),
        _ => Arc::new(ExponentialBackoffPlugin::new(cfg.clone())),
    }
}

/// 构建并发策略插件
pub fn build_concurrency_strategy(
    cfg: &core_api::ConcurrencyConfig,
) -> Arc<dyn ConcurrencyStrategyPlugin> {
    match cfg.strategy.as_str() {
        "fixed" => Arc::new(FixedConcurrencyPlugin::new(cfg.base_concurrency)),
        _ => Arc::new(AdaptiveConcurrencyPlugin::new(cfg.clone())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_task_processors_order() {
        let cfg = core_api::ExecutionConfig {
            file_processing: core_api::FileProcessingConfig {
                enabled: true,
                ..Default::default()
            },
            ..Default::default()
        };

        let processors = build_task_processors(&cfg);
        let names: Vec<String> = processors.iter().map(|p| p.name().to_string()).collect();

        assert_eq!(names[0], "file-processor");
        assert_eq!(names[1], "context-injector");
        assert_eq!(names[2], "prompt-enhancer");
    }

    #[test]
    fn test_build_renderer_jsonl() {
        let cfg = core_api::OutputConfig {
            format: "jsonl".to_string(),
            pretty_print: false,
            ascii_only: false,
        };
        let renderer = build_renderer("jsonl", &cfg);
        assert_eq!(renderer.name(), "jsonl-renderer");
    }

    #[test]
    fn test_build_retry_strategy_linear() {
        let cfg = core_api::RetryConfig {
            strategy: "linear".to_string(),
            base_delay_ms: 10,
            max_delay_ms: 100,
            max_attempts: 2,
        };
        let strategy = build_retry_strategy(&cfg);
        assert_eq!(strategy.name(), "linear");
    }

    #[test]
    fn test_build_concurrency_strategy_fixed() {
        let cfg = core_api::ConcurrencyConfig {
            strategy: "fixed".to_string(),
            min_concurrency: 1,
            max_concurrency: 8,
            base_concurrency: 3,
            cpu_threshold_low: 30.0,
            cpu_threshold_high: 80.0,
        };
        let strategy = build_concurrency_strategy(&cfg);
        assert_eq!(strategy.name(), "fixed");
    }
}
