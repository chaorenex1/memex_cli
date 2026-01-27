//! Sync CLI commands implementation
use crate::commands::cli::{SyncArgs, SyncCommand, SyncConflictsArgs, SyncNowArgs, SyncStatusArgs};
use memex_core::api as core_api;
use memex_plugins::memory::hybrid::{HybridMemoryConfig, HybridMemoryPlugin};
use memex_plugins::memory::local::{EmbeddingConfig, LocalMemoryConfig};
use memex_plugins::memory::sync::SyncConfig;
use serde_json::{json, Value};

/// Handle sync command dispatcher
pub async fn handle_sync(
    args: SyncArgs,
    ctx: &core_api::AppContext,
) -> Result<(), core_api::CliError> {
    match args.command {
        SyncCommand::Status(status_args) => handle_sync_status(status_args, ctx).await,
        SyncCommand::Now(now_args) => handle_sync_now(now_args, ctx).await,
        SyncCommand::Conflicts(conflicts_args) => handle_sync_conflicts(conflicts_args, ctx).await,
    }
}

/// Build a HybridMemoryPlugin from config for sync operations.
async fn build_hybrid_plugin(
    ctx: &core_api::AppContext,
) -> Result<HybridMemoryPlugin, core_api::CliError> {
    let cfg = ctx.cfg();

    let hybrid_cfg = match &cfg.memory.provider {
        core_api::MemoryProvider::Hybrid(h) => h,
        _ => {
            return Err(core_api::CliError::Command(
                "Sync operations require Hybrid memory provider".to_string(),
            ))
        }
    };

    // Build embedding config from local config
    let embedding = match &hybrid_cfg.local.embedding.provider {
        core_api::EmbeddingProvider::Ollama => {
            let ollama = hybrid_cfg.local.embedding.ollama.as_ref().ok_or_else(|| {
                core_api::CliError::Command("Ollama configuration is required".to_string())
            })?;
            EmbeddingConfig::Ollama {
                base_url: ollama.base_url.clone(),
                model: ollama.model.clone(),
                dimension: ollama.dimension,
            }
        }
        core_api::EmbeddingProvider::OpenAI => {
            let openai = hybrid_cfg.local.embedding.openai.as_ref().ok_or_else(|| {
                core_api::CliError::Command("OpenAI configuration is required".to_string())
            })?;
            EmbeddingConfig::OpenAI {
                base_url: openai.base_url.clone(),
                api_key: openai.api_key.clone(),
                model: openai.model.clone(),
            }
        }
        core_api::EmbeddingProvider::Local => {
            return Err(core_api::CliError::Command(
                "Local embedding provider is not supported. Please use Ollama or OpenAI."
                    .to_string(),
            ))
        }
    };

    // Expand home directory
    let db_path = shellexpand::tilde(&hybrid_cfg.local.db_path).to_string();

    // Build sync config
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

    HybridMemoryPlugin::new(hybrid_config)
        .await
        .map_err(|e| core_api::CliError::Command(format!("Failed to create hybrid plugin: {}", e)))
}

/// Handle sync status command
async fn handle_sync_status(
    args: SyncStatusArgs,
    ctx: &core_api::AppContext,
) -> Result<(), core_api::CliError> {
    let cfg = ctx.cfg();

    // Check if hybrid memory is configured
    let sync_status: Value = match &cfg.memory.provider {
        core_api::MemoryProvider::Hybrid(hybrid_cfg) => {
            // Try to build plugin and get actual status
            let plugin_result = build_hybrid_plugin(ctx).await;
            match plugin_result {
                Ok(plugin) => match plugin.sync_status() {
                    Some(s) => json!({
                        "provider": "hybrid",
                        "remote_url": hybrid_cfg.remote.base_url,
                        "sync_enabled": true,
                        "status": "active",
                        "is_syncing": s.sync_in_progress,
                        "last_sync_at": s.last_sync_at.map(|dt| dt.to_rfc3339()),
                        "pending_upload": s.pending_upload,
                        "pending_conflicts": s.pending_conflicts,
                        "is_online": s.is_online,
                    }),
                    None => json!({
                        "provider": "hybrid",
                        "remote_url": hybrid_cfg.remote.base_url,
                        "sync_enabled": false,
                        "status": "disabled",
                        "message": "Sync is not enabled in configuration"
                    }),
                },
                Err(_) => json!({
                    "provider": "hybrid",
                    "remote_url": hybrid_cfg.remote.base_url,
                    "status": "error",
                    "message": "Failed to access sync service"
                }),
            }
        }
        core_api::MemoryProvider::Local(local_cfg) => {
            json!({
                "provider": "local",
                "db_path": local_cfg.db_path,
                "sync_enabled": local_cfg.sync.enabled,
                "status": "local_only",
                "message": "Local memory does not support sync"
            })
        }
        core_api::MemoryProvider::Service(service_cfg) => {
            json!({
                "provider": "service",
                "base_url": service_cfg.base_url,
                "status": "remote_only",
                "message": "Service memory does not support local sync"
            })
        }
    };

    match args.format.as_str() {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&sync_status).unwrap());
        }
        "markdown" => {
            println!("### Sync Status\n");
            println!("**Provider**: {}\n", sync_status["provider"]);
            if let Some(Value::String(url)) = sync_status.get("remote_url") {
                println!("**Remote URL**: {}\n", url);
            }
            if let Some(Value::Bool(enabled)) = sync_status.get("sync_enabled") {
                println!("**Sync Enabled**: {}\n", enabled);
            }
            if let Some(Value::String(status)) = sync_status.get("status") {
                println!("**Status**: {}\n", status);
            }
            if let Some(Value::Bool(true)) = sync_status.get("is_syncing") {
                println!("**Currently Syncing**: Yes\n");
            }
            if let Some(Value::String(last_sync)) = sync_status.get("last_sync_at") {
                println!("**Last Sync**: {}\n", last_sync);
            } else {
                println!("**Last Sync**: Never\n");
            }
            if let Some(pending) = sync_status.get("pending_upload") {
                println!("**Pending Upload**: {}\n", pending);
            }
        }
        _ => {
            return Err(core_api::CliError::Command(format!(
                "Unknown format: {}",
                args.format
            )));
        }
    }

    Ok(())
}

/// Handle sync now command
async fn handle_sync_now(
    args: SyncNowArgs,
    ctx: &core_api::AppContext,
) -> Result<(), core_api::CliError> {
    let cfg = ctx.cfg();

    // Check if hybrid memory is configured
    match &cfg.memory.provider {
        core_api::MemoryProvider::Hybrid(_) => {
            // Build plugin to access sync functionality
            let plugin = build_hybrid_plugin(ctx).await?;

            if !plugin.is_sync_enabled() {
                return Err(core_api::CliError::Command(
                    "Sync is not enabled in configuration".to_string(),
                ));
            }

            if args.wait {
                println!("Triggering sync and waiting for completion...");
                plugin.trigger_sync();

                // Wait for sync to complete (poll status)
                let mut synced = false;
                for _ in 0..60 {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    if let Some(status) = plugin.sync_status() {
                        if !status.sync_in_progress && status.last_sync_at.is_some() {
                            synced = true;
                            break;
                        }
                    }
                }

                if synced {
                    println!("Sync completed.");
                } else {
                    println!("Sync may still be in progress or failed to complete.");
                }
            } else {
                println!("Sync triggered in background.");
                plugin.trigger_sync();
            }
        }
        core_api::MemoryProvider::Local(_) => {
            return Err(core_api::CliError::Command(
                "Local memory does not support sync".to_string(),
            ));
        }
        core_api::MemoryProvider::Service(_) => {
            return Err(core_api::CliError::Command(
                "Service memory does not support local sync".to_string(),
            ));
        }
    }

    let output = json!({
        "success": true,
        "message": "Sync triggered",
        "wait": args.wait
    });
    println!("{}", serde_json::to_string_pretty(&output).unwrap());

    Ok(())
}

/// Handle sync conflicts command
async fn handle_sync_conflicts(
    args: SyncConflictsArgs,
    ctx: &core_api::AppContext,
) -> Result<(), core_api::CliError> {
    let cfg = ctx.cfg();

    // Check if hybrid memory is configured
    let conflicts = match &cfg.memory.provider {
        core_api::MemoryProvider::Hybrid(_) => {
            // Build plugin to access conflicts
            match build_hybrid_plugin(ctx).await {
                Ok(plugin) => {
                    let conflict_list = plugin.get_conflicts().await.map_err(|e| {
                        core_api::CliError::Command(format!("Failed to get conflicts: {}", e))
                    })?;

                    json!({
                        "provider": "hybrid",
                        "conflicts": conflict_list,
                        "count": conflict_list.len(),
                    })
                }
                Err(e) => {
                    json!({
                        "provider": "hybrid",
                        "conflicts": [],
                        "count": 0,
                        "error": e.to_string()
                    })
                }
            }
        }
        core_api::MemoryProvider::Local(_) => {
            json!({
                "provider": "local",
                "conflicts": [],
                "count": 0,
                "message": "Local memory does not have conflicts"
            })
        }
        core_api::MemoryProvider::Service(_) => {
            json!({
                "provider": "service",
                "conflicts": [],
                "count": 0,
                "message": "Service memory does not have conflicts"
            })
        }
    };

    match args.format.as_str() {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&conflicts).unwrap());
        }
        "markdown" => {
            println!("### Sync Conflicts\n");
            println!("**Count**: {}\n", conflicts["count"]);
            if let Some(msg) = conflicts.get("message") {
                println!("**Message**: {}\n", msg.as_str().unwrap_or(""));
            }
            if let Some(conflict_list) = conflicts.get("conflicts") {
                if let Some(arr) = conflict_list.as_array() {
                    if arr.is_empty() {
                        println!("No pending conflicts.");
                    } else {
                        for (i, conflict) in arr.iter().enumerate() {
                            println!("**Conflict {}**:\n```\n{}\n```\n", i + 1, conflict);
                        }
                    }
                }
            }
        }
        _ => {
            return Err(core_api::CliError::Command(format!(
                "Unknown format: {}",
                args.format
            )));
        }
    }

    Ok(())
}
