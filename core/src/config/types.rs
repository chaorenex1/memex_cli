use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use crate::executor::types::ExecutionConfig;

/// Backend execution strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BackendKind {
    #[default]
    Codecli,
    Aiservice,
}

impl fmt::Display for BackendKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BackendKind::Codecli => write!(f, "codecli"),
            BackendKind::Aiservice => write!(f, "aiservice"),
        }
    }
}

impl FromStr for BackendKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "codecli" => Ok(BackendKind::Codecli),
            "aiservice" => Ok(BackendKind::Aiservice),
            _ => Err(format!("Unknown backend kind: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub backend_kind: BackendKind,

    #[serde(default)]
    pub env_file: String,

    #[serde(default)]
    pub logging: LoggingConfig,

    #[serde(default)]
    pub tui: TuiConfig,

    #[serde(default)]
    pub control: ControlConfig,

    #[serde(default)]
    pub policy: PolicyConfig,

    #[serde(default)]
    pub memory: MemoryConfig,

    #[serde(default)]
    pub prompt_inject: PromptInjectConfig,

    #[serde(default)]
    pub candidate_extract: CandidateExtractConfig,

    #[serde(default)]
    pub runner: RunnerConfig,

    #[serde(default)]
    pub events_out: EventsOutConfig,

    #[serde(default)]
    pub gatekeeper: GatekeeperConfig,

    #[serde(default)]
    pub http_server: HttpServerConfig,

    #[serde(default)]
    pub stdio: StdioConfig,

    #[serde(default)]
    pub executor: ExecutionConfig,
}

fn default_env_file() -> String {
    ".env".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            backend_kind: BackendKind::default(),
            env_file: default_env_file(),
            logging: LoggingConfig::default(),
            tui: TuiConfig::default(),
            control: ControlConfig::default(),
            policy: PolicyConfig::default(),
            memory: MemoryConfig::default(),
            prompt_inject: PromptInjectConfig::default(),
            candidate_extract: CandidateExtractConfig::default(),
            runner: RunnerConfig::default(),
            events_out: EventsOutConfig::default(),
            gatekeeper: GatekeeperConfig::default(),
            http_server: HttpServerConfig::default(),
            stdio: StdioConfig::default(),
            executor: ExecutionConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_logging_enabled")]
    pub enabled: bool,

    /// If true, log to stderr.
    #[serde(default = "default_logging_console")]
    pub console: bool,

    /// If true, log to a file under `directory` (or OS temp dir if unset).
    #[serde(default = "default_logging_file")]
    pub file: bool,

    /// EnvFilter string, e.g. "info" or "memex_core=debug".
    #[serde(default = "default_logging_level")]
    pub level: String,

    /// Optional directory for log files. If empty or unset, uses OS temp dir.
    #[serde(default)]
    pub directory: Option<String>,
}

fn default_logging_enabled() -> bool {
    true
}

fn default_logging_console() -> bool {
    true
}

fn default_logging_file() -> bool {
    true
}

fn default_logging_level() -> String {
    "info".to_string()
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            enabled: default_logging_enabled(),
            console: default_logging_console(),
            file: default_logging_file(),
            level: default_logging_level(),
            directory: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiConfig {
    #[serde(default = "default_tui_enabled")]
    pub enabled: bool,
    #[serde(default = "default_tui_auto_scroll")]
    pub auto_scroll: bool,
    #[serde(default = "default_tui_show_splash")]
    pub show_splash: bool,
    #[serde(default = "default_tui_splash_duration_ms")]
    pub splash_duration_ms: u64,
    #[serde(default = "default_tui_splash_animation")]
    pub splash_animation: bool,
    #[serde(default = "default_tui_update_interval_ms")]
    pub update_interval_ms: u64,
    #[serde(default = "default_tui_max_tool_events")]
    pub max_tool_events: usize,
    #[serde(default = "default_tui_max_output_lines")]
    pub max_output_lines: usize,
}

fn default_tui_enabled() -> bool {
    true
}

fn default_tui_auto_scroll() -> bool {
    true
}

fn default_tui_show_splash() -> bool {
    true
}

fn default_tui_splash_duration_ms() -> u64 {
    1500
}

fn default_tui_splash_animation() -> bool {
    true
}

fn default_tui_update_interval_ms() -> u64 {
    50
}

fn default_tui_max_tool_events() -> usize {
    1000
}

fn default_tui_max_output_lines() -> usize {
    10000
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            enabled: default_tui_enabled(),
            auto_scroll: default_tui_auto_scroll(),
            show_splash: default_tui_show_splash(),
            splash_duration_ms: default_tui_splash_duration_ms(),
            splash_animation: default_tui_splash_animation(),
            update_interval_ms: default_tui_update_interval_ms(),
            max_tool_events: default_tui_max_tool_events(),
            max_output_lines: default_tui_max_output_lines(),
        }
    }
}

impl AppConfig {
    // NOTE: gatekeeper 逻辑配置的转换实现迁移到 crate::gatekeeper 模块，
    // 以避免 core::config 反向依赖业务模块。
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventsOutConfig {
    pub enabled: bool,
    pub path: String,
    pub channel_capacity: usize,
    pub drop_when_full: bool,
}

impl Default for EventsOutConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            path: "./run.events.jsonl".to_string(),
            channel_capacity: 2048,
            drop_when_full: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlConfig {
    #[serde(default = "default_fail_mode")]
    pub fail_mode: String,

    #[serde(default = "default_decision_timeout_ms")]
    pub decision_timeout_ms: u64,

    #[serde(default = "default_abort_grace_ms")]
    pub abort_grace_ms: u64,

    // Runner internal buffering/timing knobs (keep defaults aligned with current constants)
    #[serde(default = "default_line_tap_channel_capacity")]
    pub line_tap_channel_capacity: usize,

    #[serde(default = "default_control_channel_capacity")]
    pub control_channel_capacity: usize,

    #[serde(default = "default_control_writer_error_capacity")]
    pub control_writer_error_capacity: usize,

    #[serde(default = "default_tick_interval_ms")]
    pub tick_interval_ms: u64,
}

fn default_fail_mode() -> String {
    "closed".to_string()
}

fn default_decision_timeout_ms() -> u64 {
    300_000
}

fn default_abort_grace_ms() -> u64 {
    5_000
}

fn default_line_tap_channel_capacity() -> usize {
    1024
}

fn default_control_channel_capacity() -> usize {
    128
}

fn default_control_writer_error_capacity() -> usize {
    1
}

fn default_tick_interval_ms() -> u64 {
    1_000
}

impl Default for ControlConfig {
    fn default() -> Self {
        Self {
            fail_mode: default_fail_mode(),
            decision_timeout_ms: default_decision_timeout_ms(),
            abort_grace_ms: default_abort_grace_ms(),
            line_tap_channel_capacity: default_line_tap_channel_capacity(),
            control_channel_capacity: default_control_channel_capacity(),
            control_writer_error_capacity: default_control_writer_error_capacity(),
            tick_interval_ms: default_tick_interval_ms(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyConfig {
    #[serde(default = "default_policy_provider")]
    #[serde(flatten)]
    pub provider: PolicyProvider,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "provider")]
pub enum PolicyProvider {
    #[serde(rename = "config")]
    Config(ConfigPolicyConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigPolicyConfig {
    #[serde(default = "default_policy_mode")]
    pub mode: String,

    #[serde(default = "default_policy_action")]
    pub default_action: String,

    #[serde(default)]
    pub allowlist: Vec<PolicyRule>,

    #[serde(default = "default_denylist")]
    pub denylist: Vec<PolicyRule>,
}

fn default_policy_provider() -> PolicyProvider {
    PolicyProvider::Config(ConfigPolicyConfig::default())
}

fn default_policy_mode() -> String {
    "auto".to_string()
}

fn default_policy_action() -> String {
    "deny".to_string()
}

fn default_denylist() -> Vec<PolicyRule> {
    vec![
        PolicyRule {
            tool: "shell.exec".into(),
            action: Some("exec".into()),
            reason: Some("shell is denied by default".into()),
        },
        PolicyRule {
            tool: "net.http".into(),
            action: Some("net".into()),
            reason: Some("network is denied by default".into()),
        },
    ]
}

impl Default for ConfigPolicyConfig {
    fn default() -> Self {
        Self {
            mode: default_policy_mode(),
            default_action: default_policy_action(),
            allowlist: vec![
                PolicyRule {
                    tool: "fs.read".into(),
                    action: Some("read".into()),
                    reason: Some("read is allowed".into()),
                },
                PolicyRule {
                    tool: "git.*".into(),
                    action: None,
                    reason: Some("git commands allowed".into()),
                },
            ],
            denylist: default_denylist(),
        }
    }
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            provider: default_policy_provider(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    pub tool: String,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    #[serde(default = "default_memory_enabled")]
    pub enabled: bool,

    #[serde(flatten)]
    pub provider: MemoryProvider,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "provider")]
pub enum MemoryProvider {
    #[serde(rename = "service")]
    Service(MemoryServiceConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryServiceConfig {
    #[serde(default = "default_memory_url")]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,

    #[serde(default = "default_search_limit")]
    pub search_limit: u32,
    #[serde(default = "default_min_score")]
    pub min_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PromptInjectPlacement {
    System,
    User,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptInjectConfig {
    #[serde(default = "default_prompt_inject_placement")]
    pub placement: PromptInjectPlacement,
    #[serde(default = "default_prompt_inject_max_items")]
    pub max_items: usize,
    #[serde(default = "default_prompt_inject_max_answer_chars")]
    pub max_answer_chars: usize,
    #[serde(default = "default_prompt_inject_include_meta_line")]
    pub include_meta_line: bool,
}

fn default_prompt_inject_placement() -> PromptInjectPlacement {
    PromptInjectPlacement::System
}

fn default_prompt_inject_max_items() -> usize {
    3
}

fn default_prompt_inject_max_answer_chars() -> usize {
    900
}

fn default_prompt_inject_include_meta_line() -> bool {
    true
}

impl Default for PromptInjectConfig {
    fn default() -> Self {
        Self {
            placement: default_prompt_inject_placement(),
            max_items: default_prompt_inject_max_items(),
            max_answer_chars: default_prompt_inject_max_answer_chars(),
            include_meta_line: default_prompt_inject_include_meta_line(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateExtractConfig {
    #[serde(default = "default_candidate_extract_max_candidates")]
    pub max_candidates: usize,
    #[serde(default = "default_candidate_extract_max_answer_chars")]
    pub max_answer_chars: usize,
    #[serde(default = "default_candidate_extract_min_answer_chars")]
    pub min_answer_chars: usize,
    #[serde(default = "default_candidate_extract_context_lines")]
    pub context_lines: usize,
    #[serde(default = "default_candidate_extract_tool_steps_max")]
    pub tool_steps_max: usize,
    #[serde(default = "default_candidate_extract_tool_step_args_keys_max")]
    pub tool_step_args_keys_max: usize,
    #[serde(default = "default_candidate_extract_tool_step_value_max_chars")]
    pub tool_step_value_max_chars: usize,
    #[serde(default = "default_candidate_extract_redact")]
    pub redact: bool,
    #[serde(default = "default_candidate_extract_strict_secret_block")]
    pub strict_secret_block: bool,
    #[serde(default = "default_candidate_extract_confidence")]
    pub confidence: f32,
}

fn default_candidate_extract_max_candidates() -> usize {
    1
}

fn default_candidate_extract_max_answer_chars() -> usize {
    1200
}

fn default_candidate_extract_min_answer_chars() -> usize {
    200
}

fn default_candidate_extract_context_lines() -> usize {
    8
}

fn default_candidate_extract_tool_steps_max() -> usize {
    5
}

fn default_candidate_extract_tool_step_args_keys_max() -> usize {
    16
}

fn default_candidate_extract_tool_step_value_max_chars() -> usize {
    140
}

fn default_candidate_extract_redact() -> bool {
    true
}

fn default_candidate_extract_strict_secret_block() -> bool {
    true
}

fn default_candidate_extract_confidence() -> f32 {
    0.45
}

impl Default for CandidateExtractConfig {
    fn default() -> Self {
        Self {
            max_candidates: default_candidate_extract_max_candidates(),
            max_answer_chars: default_candidate_extract_max_answer_chars(),
            min_answer_chars: default_candidate_extract_min_answer_chars(),
            context_lines: default_candidate_extract_context_lines(),
            tool_steps_max: default_candidate_extract_tool_steps_max(),
            tool_step_args_keys_max: default_candidate_extract_tool_step_args_keys_max(),
            tool_step_value_max_chars: default_candidate_extract_tool_step_value_max_chars(),
            redact: default_candidate_extract_redact(),
            strict_secret_block: default_candidate_extract_strict_secret_block(),
            confidence: default_candidate_extract_confidence(),
        }
    }
}

fn default_memory_enabled() -> bool {
    true
}

fn default_memory_url() -> String {
    "https://memory.internal".to_string()
}

fn default_timeout_ms() -> u64 {
    10_000
}

fn default_search_limit() -> u32 {
    6
}

fn default_min_score() -> f32 {
    0.2
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            enabled: default_memory_enabled(),
            provider: MemoryProvider::Service(MemoryServiceConfig {
                base_url: default_memory_url(),
                api_key: "".to_string(),
                timeout_ms: default_timeout_ms(),
                search_limit: default_search_limit(),
                min_score: default_min_score(),
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "provider")]
pub enum RunnerConfig {
    #[serde(rename = "codecli")]
    CodeCli(CodeCliRunnerConfig),
    #[serde(rename = "replay")]
    Replay(ReplayRunnerConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReplayRunnerConfig {
    pub events_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CodeCliRunnerConfig {
    // Local runner configuration fields can be added here
}

impl Default for RunnerConfig {
    fn default() -> Self {
        RunnerConfig::CodeCli(CodeCliRunnerConfig::default())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatekeeperConfig {
    #[serde(default = "default_gatekeeper_provider")]
    #[serde(flatten)]
    pub provider: GatekeeperProvider,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "provider")]
pub enum GatekeeperProvider {
    #[serde(rename = "standard")]
    Standard(StandardGatekeeperConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandardGatekeeperConfig {
    #[serde(default = "default_max_inject")]
    pub max_inject: usize,
    #[serde(default = "default_min_level_inject")]
    pub min_level_inject: i32,
    #[serde(default = "default_min_level_fallback")]
    pub min_level_fallback: i32,
    #[serde(default = "default_min_trust_show")]
    pub min_trust_show: f32,
    #[serde(default = "default_block_if_consecutive_fail_ge")]
    pub block_if_consecutive_fail_ge: i32,
    #[serde(default = "default_skip_if_top1_score_ge")]
    pub skip_if_top1_score_ge: f32,
    #[serde(default = "default_exclude_stale_by_default")]
    pub exclude_stale_by_default: bool,
    #[serde(default = "default_active_statuses")]
    pub active_statuses: std::collections::HashSet<String>,

    #[serde(default = "default_gatekeeper_digest_head_chars")]
    pub digest_head_chars: usize,
    #[serde(default = "default_gatekeeper_digest_tail_chars")]
    pub digest_tail_chars: usize,
}

// NOTE: Gatekeeper 配置的转换实现迁移到 crate::gatekeeper 模块，
// 以避免 core::config 反向依赖业务模块。

fn default_max_inject() -> usize {
    3
}
fn default_min_level_inject() -> i32 {
    2
}
fn default_min_level_fallback() -> i32 {
    1
}
fn default_min_trust_show() -> f32 {
    0.40
}
fn default_block_if_consecutive_fail_ge() -> i32 {
    3
}
fn default_skip_if_top1_score_ge() -> f32 {
    0.85
}
fn default_exclude_stale_by_default() -> bool {
    true
}
fn default_active_statuses() -> std::collections::HashSet<String> {
    ["active".to_string(), "verified".to_string()]
        .into_iter()
        .collect()
}

fn default_gatekeeper_digest_head_chars() -> usize {
    80
}

fn default_gatekeeper_digest_tail_chars() -> usize {
    80
}

fn default_gatekeeper_provider() -> GatekeeperProvider {
    GatekeeperProvider::Standard(StandardGatekeeperConfig::default())
}

impl Default for StandardGatekeeperConfig {
    fn default() -> Self {
        Self {
            max_inject: default_max_inject(),
            min_level_inject: default_min_level_inject(),
            min_level_fallback: default_min_level_fallback(),
            min_trust_show: default_min_trust_show(),
            block_if_consecutive_fail_ge: default_block_if_consecutive_fail_ge(),
            skip_if_top1_score_ge: default_skip_if_top1_score_ge(),
            exclude_stale_by_default: default_exclude_stale_by_default(),
            active_statuses: default_active_statuses(),
            digest_head_chars: default_gatekeeper_digest_head_chars(),
            digest_tail_chars: default_gatekeeper_digest_tail_chars(),
        }
    }
}

impl Default for GatekeeperConfig {
    fn default() -> Self {
        Self {
            provider: default_gatekeeper_provider(),
        }
    }
}

// ============= HTTP Server Config =============

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpServerConfig {
    #[serde(default = "default_http_server_host")]
    pub host: String,

    #[serde(default = "default_http_server_port")]
    pub port: u16,
}

fn default_http_server_host() -> String {
    "127.0.0.1".to_string()
}

fn default_http_server_port() -> u16 {
    8080
}

impl Default for HttpServerConfig {
    fn default() -> Self {
        Self {
            host: default_http_server_host(),
            port: default_http_server_port(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StdioConfig {
    /// 最大并行任务数
    #[serde(default = "default_max_parallel_tasks")]
    pub max_parallel_tasks: usize,

    /// 启用自适应并发调度（Level 2.2）
    #[serde(default = "default_enable_adaptive_concurrency")]
    pub enable_adaptive_concurrency: bool,

    /// 启用事件批量化输出（Level 2.1）
    #[serde(default = "default_enable_event_buffering")]
    pub enable_event_buffering: bool,

    /// 事件缓冲区大小
    #[serde(default = "default_event_buffer_size")]
    pub event_buffer_size: usize,

    /// 事件刷新间隔（毫秒）
    #[serde(default = "default_event_flush_interval_ms")]
    pub event_flush_interval_ms: u64,

    /// 启用文件缓存（Level 3.3）
    #[serde(default = "default_enable_file_cache")]
    pub enable_file_cache: bool,

    /// 文件缓存大小
    #[serde(default = "default_file_cache_size")]
    pub file_cache_size: usize,

    /// 启用大文件内存映射（Level 3.1）
    #[serde(default = "default_enable_mmap_large_files")]
    pub enable_mmap_large_files: bool,

    /// 内存映射阈值（MB）
    #[serde(default = "default_mmap_threshold_mb")]
    pub mmap_threshold_mb: u64,
}

fn default_max_parallel_tasks() -> usize {
    // Tiered default based on CPU count (Strategy C)
    let cpu_count = num_cpus::get();
    match cpu_count {
        1..=2 => 2,             // Low-end devices
        3..=8 => cpu_count / 2, // Personal computers
        9..=16 => 6,            // Workstations
        _ => 8,                 // Servers (cap to avoid overload)
    }
}

fn default_enable_adaptive_concurrency() -> bool {
    true
}

fn default_enable_event_buffering() -> bool {
    true
}

fn default_event_buffer_size() -> usize {
    50
}

fn default_event_flush_interval_ms() -> u64 {
    100
}

fn default_enable_file_cache() -> bool {
    false // 默认关闭，避免内存占用
}

fn default_file_cache_size() -> usize {
    100
}

fn default_enable_mmap_large_files() -> bool {
    true
}

fn default_mmap_threshold_mb() -> u64 {
    10
}

impl Default for StdioConfig {
    fn default() -> Self {
        Self {
            max_parallel_tasks: default_max_parallel_tasks(),
            enable_adaptive_concurrency: default_enable_adaptive_concurrency(),
            enable_event_buffering: default_enable_event_buffering(),
            event_buffer_size: default_event_buffer_size(),
            event_flush_interval_ms: default_event_flush_interval_ms(),
            enable_file_cache: default_enable_file_cache(),
            file_cache_size: default_file_cache_size(),
            enable_mmap_large_files: default_enable_mmap_large_files(),
            mmap_threshold_mb: default_mmap_threshold_mb(),
        }
    }
}
