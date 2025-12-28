use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_project_id")]
    pub project_id: String,

    #[serde(default)]
    pub logging: LoggingConfig,

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
}

fn default_project_id() -> String {
    "my-project".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            project_id: default_project_id(),
            logging: LoggingConfig::default(),
            control: ControlConfig::default(),
            policy: PolicyConfig::default(),
            memory: MemoryConfig::default(),
            prompt_inject: PromptInjectConfig::default(),
            candidate_extract: CandidateExtractConfig::default(),
            runner: RunnerConfig::default(),
            events_out: EventsOutConfig::default(),
            gatekeeper: GatekeeperConfig::default(),
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
            enabled: true,
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
