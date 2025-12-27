use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub project_id: String,

    #[serde(default)]
    pub control: ControlConfig,

    #[serde(default)]
    pub policy: PolicyConfig,

    #[serde(default)]
    pub memory: MemoryConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlConfig {
    #[serde(default = "default_fail_mode")]
    pub fail_mode: String, // "closed" | "open"

    #[serde(default = "default_decision_timeout_ms")]
    pub decision_timeout_ms: u64,

    #[serde(default = "default_abort_grace_ms")]
    pub abort_grace_ms: u64,
}

fn default_fail_mode() -> String { "closed".to_string() }
fn default_decision_timeout_ms() -> u64 { 300_000 }
fn default_abort_grace_ms() -> u64 { 5_000 }

impl Default for ControlConfig {
    fn default() -> Self {
        Self {
            fail_mode: default_fail_mode(),
            decision_timeout_ms: default_decision_timeout_ms(),
            abort_grace_ms: default_abort_grace_ms(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PolicyConfig {
    #[serde(default)]
    pub mode: String, // "off" | "auto" | "prompt"（本轮先实现 auto/off）

    #[serde(default)]
    pub default_action: String, // "allow" | "deny"

    #[serde(default)]
    pub allowlist: Vec<PolicyRule>,

    #[serde(default)]
    pub denylist: Vec<PolicyRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    pub tool: String,             // 支持前缀或 "*" 通配（简单版）
    #[serde(default)]
    pub action: Option<String>,   // read|write|net|exec
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,

    // search defaults
    #[serde(default = "default_search_limit")]
    pub search_limit: u32,
    #[serde(default = "default_min_score")]
    pub min_score: f32,
}

fn default_timeout_ms() -> u64 { 10_000 }
fn default_search_limit() -> u32 { 6 }
fn default_min_score() -> f32 { 0.2 }

pub fn load_default() -> anyhow::Result<AppConfig> {
    let mut cfg = if Path::new(".config.toml").exists() {
        let s = std::fs::read_to_string(".config.toml")?;
        toml::from_str::<AppConfig>(&s)?
    } else {
        AppConfig::default()
    };

    // env overrides (minimal)
    if let Ok(v) = std::env::var("MEM_CODECLI_PROJECT_ID") {
        if !v.trim().is_empty() { cfg.project_id = v; }
    }
    if let Ok(v) = std::env::var("MEM_CODECLI_MEMORY_URL") {
        if !v.trim().is_empty() { cfg.memory.base_url = v; }
    }
    if let Ok(v) = std::env::var("MEM_CODECLI_MEMORY_API_KEY") {
        if !v.trim().is_empty() { cfg.memory.api_key = v; }
    }

    Ok(cfg)
}
