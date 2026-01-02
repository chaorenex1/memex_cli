use std::path::{Path, PathBuf};

use super::types::{AppConfig, MemoryProvider};

/// Get the default memex data directory: ~/.memex
pub fn get_memex_data_dir() -> anyhow::Result<PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| anyhow::anyhow!("Cannot determine home directory"))?;
    Ok(PathBuf::from(home).join(".memex"))
}

pub fn get_memex_env_file_path() -> anyhow::Result<PathBuf> {
    let memex_dir = get_memex_data_dir()?;
    Ok(memex_dir.join(".env"))
}

pub fn load_default() -> anyhow::Result<AppConfig> {
    // Priority 1: ~/.memex/config.toml (highest)
    let memex_dir = get_memex_data_dir()?;
    let memex_config = memex_dir.join("config.toml");

    // Priority 2: ./config.toml (current directory)
    let local_config = Path::new("config.toml");

    let mut cfg: AppConfig = if memex_config.exists() {
        let s = std::fs::read_to_string(&memex_config)?;
        toml::from_str::<AppConfig>(&s)?
    } else if local_config.exists() {
        let s = std::fs::read_to_string(local_config)?;
        toml::from_str::<AppConfig>(&s)?
    } else {
        AppConfig::default()
    };

    cfg.env_file = get_memex_env_file_path()?.to_string_lossy().to_string();

    // Update events_out path to use memex data directory if using default
    if cfg.events_out.path == "./run.events.jsonl" {
        let events_dir = memex_dir.join("events_out");
        std::fs::create_dir_all(&events_dir)?;
        cfg.events_out.path = events_dir
            .join("run.events.jsonl")
            .to_string_lossy()
            .to_string();
    }

    // Update logging directory to use memex data directory if not set
    if cfg.logging.directory.is_none()
        || cfg
            .logging
            .directory
            .as_ref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(false)
    {
        let logs_dir = memex_dir.join("logs");
        std::fs::create_dir_all(&logs_dir)?;
        cfg.logging.directory = Some(logs_dir.to_string_lossy().to_string());
    }

    // Environment variable overrides (Priority 0: highest)
    if let Ok(v) = std::env::var("MEM_CODECLI_BACKEND_KIND") {
        if !v.trim().is_empty() {
            cfg.backend_kind = v;
        }
    }

    let MemoryProvider::Service(ref mut svc_cfg) = cfg.memory.provider;

    if let Ok(v) = std::env::var("MEM_CODECLI_MEMORY_URL") {
        if !v.trim().is_empty() {
            svc_cfg.base_url = v;
        }
    }
    if let Ok(v) = std::env::var("MEM_CODECLI_MEMORY_API_KEY") {
        if !v.trim().is_empty() {
            svc_cfg.api_key = v;
        }
    }

    Ok(cfg)
}
