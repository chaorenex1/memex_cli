//! Database CLI commands implementation
use crate::commands::cli::{DbArgs, DbCommand, DbExportArgs, DbImportArgs, DbInfoArgs, DbInitArgs};
use memex_core::api as core_api;
use memex_plugins::memory::hybrid::{HybridMemoryConfig, HybridMemoryPlugin};
use memex_plugins::memory::local::{EmbeddingConfig, LocalMemoryConfig, LocalMemoryPlugin};
use memex_plugins::memory::sync::SyncConfig;
use serde_json::json;

/// Handle db command dispatcher
pub async fn handle_db(args: DbArgs, ctx: &core_api::AppContext) -> Result<(), core_api::CliError> {
    match args.command {
        DbCommand::Init(init_args) => handle_db_init(init_args, ctx).await,
        DbCommand::Info(info_args) => handle_db_info(info_args, ctx).await,
        DbCommand::Export(export_args) => handle_db_export(export_args, ctx).await,
        DbCommand::Import(import_args) => handle_db_import(import_args, ctx).await,
    }
}

/// Build a LocalMemoryPlugin from config.
async fn build_local_plugin(
    ctx: &core_api::AppContext,
) -> Result<LocalMemoryPlugin, core_api::CliError> {
    let cfg = ctx.cfg();

    let (db_path, embedding_cfg, search_limit, min_score) = match &cfg.memory.provider {
        core_api::MemoryProvider::Local(local_cfg) => {
            let db_path = shellexpand::tilde(&local_cfg.db_path).to_string();
            let embedding = match &local_cfg.embedding.provider {
                core_api::EmbeddingProvider::Ollama => {
                    let ollama = local_cfg.embedding.ollama.as_ref().ok_or_else(|| {
                        core_api::CliError::Command("Ollama configuration is required".to_string())
                    })?;
                    EmbeddingConfig::Ollama {
                        base_url: ollama.base_url.clone(),
                        model: ollama.model.clone(),
                        dimension: ollama.dimension,
                    }
                }
                core_api::EmbeddingProvider::OpenAI => {
                    let openai = local_cfg.embedding.openai.as_ref().ok_or_else(|| {
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
            (
                db_path,
                embedding,
                local_cfg.search_limit,
                local_cfg.min_score,
            )
        }
        _ => {
            return Err(core_api::CliError::Command(
                "Local memory provider required".to_string(),
            ))
        }
    };

    let config = LocalMemoryConfig {
        db_path,
        embedding: embedding_cfg,
        search_limit,
        min_score,
    };

    LocalMemoryPlugin::new(config)
        .await
        .map_err(|e| core_api::CliError::Command(format!("Failed to create local plugin: {}", e)))
}

/// Build a HybridMemoryPlugin and get its local store.
async fn get_hybrid_store(
    ctx: &core_api::AppContext,
) -> Result<std::sync::Arc<memex_plugins::memory::lance::LanceStore>, core_api::CliError> {
    let cfg = ctx.cfg();

    let hybrid_cfg = match &cfg.memory.provider {
        core_api::MemoryProvider::Hybrid(h) => h,
        _ => {
            return Err(core_api::CliError::Command(
                "Hybrid memory provider required".to_string(),
            ))
        }
    };

    // Build embedding config
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

    let db_path = shellexpand::tilde(&hybrid_cfg.local.db_path).to_string();
    let sync_config = SyncConfig {
        enabled: false, // Don't start sync for db operations
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

    let plugin = HybridMemoryPlugin::new(hybrid_config).await.map_err(|e| {
        core_api::CliError::Command(format!("Failed to create hybrid plugin: {}", e))
    })?;

    Ok(plugin.local().store())
}

/// Handle db init command
async fn handle_db_init(
    args: DbInitArgs,
    ctx: &core_api::AppContext,
) -> Result<(), core_api::CliError> {
    let cfg = ctx.cfg();

    match &cfg.memory.provider {
        core_api::MemoryProvider::Local(local_cfg) => {
            let db_path = shellexpand::tilde(&local_cfg.db_path).to_string();

            // Check if database already exists
            if !args.force {
                let db_dir = std::path::Path::new(&db_path);
                if db_dir.exists() {
                    return Err(core_api::CliError::Command(format!(
                        "Database already exists at: {}. Use --force to reinitialize.",
                        db_path
                    )));
                }
            }

            // Create database directory
            std::fs::create_dir_all(&db_path).map_err(|e| {
                core_api::CliError::Command(format!("Failed to create database directory: {}", e))
            })?;

            println!("Database initialized at: {}", db_path);
            println!("Embedding provider: {:?}", local_cfg.embedding.provider);

            let output = json!({
                "success": true,
                "db_path": db_path,
                "embedding_provider": format!("{:?}", local_cfg.embedding.provider),
                "message": "Database initialized successfully"
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        core_api::MemoryProvider::Hybrid(hybrid_cfg) => {
            let db_path = shellexpand::tilde(&hybrid_cfg.local.db_path).to_string();

            if !args.force {
                let db_dir = std::path::Path::new(&db_path);
                if db_dir.exists() {
                    return Err(core_api::CliError::Command(format!(
                        "Database already exists at: {}. Use --force to reinitialize.",
                        db_path
                    )));
                }
            }

            std::fs::create_dir_all(&db_path).map_err(|e| {
                core_api::CliError::Command(format!("Failed to create database directory: {}", e))
            })?;

            println!("Hybrid database initialized at: {}", db_path);
            println!("Remote URL: {}", hybrid_cfg.remote.base_url);

            let output = json!({
                "success": true,
                "db_path": db_path,
                "remote_url": hybrid_cfg.remote.base_url,
                "message": "Hybrid database initialized successfully"
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        core_api::MemoryProvider::Service(_) => {
            return Err(core_api::CliError::Command(
                "Service memory does not use local database".to_string(),
            ));
        }
    }

    Ok(())
}

/// Handle db info command
async fn handle_db_info(
    args: DbInfoArgs,
    ctx: &core_api::AppContext,
) -> Result<(), core_api::CliError> {
    let cfg = ctx.cfg();

    let db_info = match &cfg.memory.provider {
        core_api::MemoryProvider::Local(local_cfg) => {
            let db_path = shellexpand::tilde(&local_cfg.db_path).to_string();
            let db_path_obj = std::path::Path::new(&db_path);

            let (exists, size_mb, item_count) = if db_path_obj.exists() {
                let size = calculate_dir_size(db_path_obj)
                    .map(|s| s / (1024 * 1024))
                    .unwrap_or(0);

                // Get actual item count from LanceStore
                let count = match build_local_plugin(ctx).await {
                    Ok(plugin) => {
                        let store = plugin.store();
                        store.count_all().await.unwrap_or(0)
                    }
                    Err(_) => 0,
                };

                (true, size, count)
            } else {
                (false, 0, 0)
            };

            json!({
                "provider": "local",
                "db_path": db_path,
                "exists": exists,
                "size_mb": size_mb,
                "item_count": item_count,
                "embedding_provider": format!("{:?}", local_cfg.embedding.provider),
                "search_limit": local_cfg.search_limit,
                "min_score": local_cfg.min_score,
            })
        }
        core_api::MemoryProvider::Hybrid(hybrid_cfg) => {
            let db_path = shellexpand::tilde(&hybrid_cfg.local.db_path).to_string();
            let db_path_obj = std::path::Path::new(&db_path);

            let (exists, size_mb, item_count) = if db_path_obj.exists() {
                let size = calculate_dir_size(db_path_obj)
                    .map(|s| s / (1024 * 1024))
                    .unwrap_or(0);

                // Get actual item count from LanceStore
                let count = match get_hybrid_store(ctx).await {
                    Ok(store) => store.count_all().await.unwrap_or(0),
                    Err(_) => 0,
                };

                (true, size, count)
            } else {
                (false, 0, 0)
            };

            json!({
                "provider": "hybrid",
                "db_path": db_path,
                "exists": exists,
                "size_mb": size_mb,
                "item_count": item_count,
                "remote_url": hybrid_cfg.remote.base_url,
                "sync_enabled": hybrid_cfg.local.sync.enabled,
                "sync_interval_secs": hybrid_cfg.local.sync.interval_secs,
            })
        }
        core_api::MemoryProvider::Service(service_cfg) => {
            json!({
                "provider": "service",
                "base_url": service_cfg.base_url,
                "message": "Service memory does not use local database"
            })
        }
    };

    match args.format.as_str() {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&db_info).unwrap());
        }
        "markdown" => {
            println!("### Database Information\n");
            println!("**Provider**: {}\n", db_info["provider"]);
            if let Some(path) = db_info.get("db_path") {
                println!("**Path**: {}\n", path.as_str().unwrap_or("N/A"));
            }
            if let Some(exists) = db_info.get("exists") {
                println!(
                    "**Exists**: {}\n",
                    if exists.as_bool().unwrap_or(false) {
                        "Yes"
                    } else {
                        "No"
                    }
                );
            }
            if let Some(size) = db_info.get("size_mb") {
                println!("**Size**: {} MB\n", size.as_u64().unwrap_or(0));
            }
            if let Some(count) = db_info.get("item_count") {
                println!("**Item Count**: {}\n", count.as_u64().unwrap_or(0));
            }
            if let Some(url) = db_info.get("remote_url") {
                println!("**Remote URL**: {}\n", url.as_str().unwrap_or("N/A"));
            }
            if let Some(msg) = db_info.get("message") {
                println!("**Message**: {}\n", msg.as_str().unwrap_or(""));
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

/// Handle db export command
async fn handle_db_export(
    args: DbExportArgs,
    ctx: &core_api::AppContext,
) -> Result<(), core_api::CliError> {
    let cfg = ctx.cfg();

    // Determine output file path
    let output_path = args.output.unwrap_or_else(|| {
        format!(
            "qa_export_{}.jsonl",
            chrono::Utc::now().format("%Y%m%d_%H%M%S")
        )
    });

    match &cfg.memory.provider {
        core_api::MemoryProvider::Local(_) => {
            let plugin = build_local_plugin(ctx).await?;
            let store = plugin.store();

            // Open output file
            let mut file = tokio::fs::File::create(&output_path).await.map_err(|e| {
                core_api::CliError::Command(format!("Failed to create output file: {}", e))
            })?;

            // Export data
            store
                .export_qa(&mut file)
                .await
                .map_err(|e| core_api::CliError::Command(format!("Export failed: {}", e)))?;

            let output = json!({
                "success": true,
                "output_file": output_path,
                "format": args.format,
                "message": "Export completed successfully"
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        core_api::MemoryProvider::Hybrid(_) => {
            let store = get_hybrid_store(ctx).await?;

            // Open output file
            let mut file = tokio::fs::File::create(&output_path).await.map_err(|e| {
                core_api::CliError::Command(format!("Failed to create output file: {}", e))
            })?;

            // Export data
            store
                .export_qa(&mut file)
                .await
                .map_err(|e| core_api::CliError::Command(format!("Export failed: {}", e)))?;

            let output = json!({
                "success": true,
                "output_file": output_path,
                "format": args.format,
                "message": "Export completed successfully"
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        core_api::MemoryProvider::Service(_) => {
            return Err(core_api::CliError::Command(
                "Service memory does not support export".to_string(),
            ));
        }
    }

    Ok(())
}

/// Handle db import command
async fn handle_db_import(
    args: DbImportArgs,
    ctx: &core_api::AppContext,
) -> Result<(), core_api::CliError> {
    let cfg = ctx.cfg();

    // Check if input file exists
    if !std::path::Path::new(&args.input).exists() {
        return Err(core_api::CliError::Command(format!(
            "Input file does not exist: {}",
            args.input
        )));
    }

    match &cfg.memory.provider {
        core_api::MemoryProvider::Local(_) => {
            let plugin = build_local_plugin(ctx).await?;
            let store = plugin.store();

            // Open input file
            let file = tokio::fs::File::open(&args.input).await.map_err(|e| {
                core_api::CliError::Command(format!("Failed to open input file: {}", e))
            })?;
            let mut reader = tokio::io::BufReader::new(file);

            // Import data
            let imported = store
                .import_qa(&mut reader, args.skip_existing)
                .await
                .map_err(|e| core_api::CliError::Command(format!("Import failed: {}", e)))?;

            let output = json!({
                "success": true,
                "input_file": args.input,
                "imported_count": imported,
                "skip_existing": args.skip_existing,
                "message": "Import completed successfully"
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        core_api::MemoryProvider::Hybrid(_) => {
            let store = get_hybrid_store(ctx).await?;

            // Open input file
            let file = tokio::fs::File::open(&args.input).await.map_err(|e| {
                core_api::CliError::Command(format!("Failed to open input file: {}", e))
            })?;
            let mut reader = tokio::io::BufReader::new(file);

            // Import data
            let imported = store
                .import_qa(&mut reader, args.skip_existing)
                .await
                .map_err(|e| core_api::CliError::Command(format!("Import failed: {}", e)))?;

            let output = json!({
                "success": true,
                "input_file": args.input,
                "imported_count": imported,
                "skip_existing": args.skip_existing,
                "message": "Import completed successfully"
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        core_api::MemoryProvider::Service(_) => {
            return Err(core_api::CliError::Command(
                "Service memory does not support import".to_string(),
            ));
        }
    }

    Ok(())
}

/// Calculate directory size recursively
fn calculate_dir_size(path: &std::path::Path) -> Result<u64, std::io::Error> {
    let mut total = 0;
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let entry_path = entry.path();
            if entry_path.is_dir() {
                total += calculate_dir_size(&entry_path)?;
            } else {
                total += entry.metadata()?.len();
            }
        }
    } else {
        total = path.metadata()?.len();
    }
    Ok(total)
}
