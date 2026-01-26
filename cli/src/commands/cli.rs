use clap::{Args as ClapArgs, Parser, Subcommand};
use serde::{Deserialize, Serialize};

fn default_stream_format() -> String {
    "text".to_string()
}

fn default_true() -> bool {
    true
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackendKind {
    Codecli,
    Aiservice,
}

impl From<BackendKind> for memex_core::api::BackendKind {
    fn from(kind: BackendKind) -> Self {
        match kind {
            BackendKind::Codecli => memex_core::api::BackendKind::Codecli,
            BackendKind::Aiservice => memex_core::api::BackendKind::Aiservice,
        }
    }
}

impl From<memex_core::api::BackendKind> for BackendKind {
    fn from(kind: memex_core::api::BackendKind) -> Self {
        match kind {
            memex_core::api::BackendKind::Codecli => BackendKind::Codecli,
            memex_core::api::BackendKind::Aiservice => BackendKind::Aiservice,
        }
    }
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TaskLevel {
    #[default]
    Auto,
    L0,
    L1,
    L2,
    L3,
}

#[derive(Parser, Debug, Clone)]
#[command(version)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Commands>,

    // #[arg(long, default_value = "codex", global = false)]
    // pub codecli_bin: String,

    // #[arg(trailing_var_arg = true, global = false)]
    // pub codecli_args: Vec<String>,
    #[arg(long, default_value_t = 65536, global = false)]
    pub capture_bytes: usize,
}

#[derive(ClapArgs, Debug, Clone, Serialize, Deserialize)]
pub struct RunArgs {
    #[arg(long)]
    pub backend: String,

    /// Explicitly select how to interpret `--backend`.
    /// - auto: URL => aiservice, otherwise => codecli
    /// - codecli: treat backend as a local binary name/path
    /// - aiservice: treat backend as an http(s) URL
    #[arg(long, value_enum)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_kind: Option<BackendKind>,

    #[arg(long)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    #[arg(long)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_provider: Option<String>,

    /// Task level for scheduling/strategy hints.
    /// - auto: infer from prompt (fast heuristic)
    /// - L0..L3: explicitly set
    #[arg(long, value_enum, default_value_t = TaskLevel::Auto)]
    #[serde(default)]
    pub task_level: TaskLevel,

    #[arg(long, group = "input")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,

    #[arg(long, group = "input")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_file: Option<String>,

    #[arg(long, group = "input")]
    #[serde(default)]
    pub stdin: bool,

    #[arg(long, default_value = "text")]
    #[serde(default = "default_stream_format")]
    pub stream_format: String,

    /// Force TUI mode (does not affect `--stream-format`).
    #[arg(long, default_value_t = false)]
    #[serde(default)]
    pub tui: bool,

    /// Extra environment variables to pass to the backend process (KEY=VALUE).
    /// Can be specified multiple times.
    #[arg(long = "env", action = clap::ArgAction::Append)]
    #[serde(default)]
    pub env: Vec<String>,

    /// Load environment variables from a file (KEY=VALUE per line).
    /// Lines starting with # are ignored. Empty lines are not allowed.
    #[arg(long = "env-file", alias = "env_file")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env_file: Option<String>,

    #[arg(long)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// Parse input as structured STDIO protocol text (default: true)
    ///
    /// When enabled (--structured-text):
    ///   - Parse input following STDIO protocol format
    ///   - Support multiple tasks with dependencies
    ///   - Input format: ---TASK--- / ---CONTENT--- / ---END---
    ///
    /// When disabled (--no-structured-text):
    ///   - Treat input as plain text content
    ///   - Create single task automatically
    ///   - Useful for simple prompts
    #[arg(long, default_value_t = true)]
    #[serde(default = "default_true")]
    pub structured_text: bool,
}

impl RunArgs {
    /// Serialize `RunArgs` into compact JSON.
    pub fn to_json_string(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Serialize `RunArgs` into pretty JSON (useful for logs / debugging).
    pub fn to_pretty_json_string(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize `RunArgs` from JSON text.
    pub fn from_json_str(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    /// Convert `RunArgs` into a JSON `Value`.
    pub fn to_json_value(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(self)
    }

    /// Deserialize `RunArgs` from a JSON `Value`.
    pub fn from_json_value(v: serde_json::Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(v)
    }
}

#[derive(ClapArgs, Debug, Clone)]
pub struct ReplayArgs {
    #[arg(long)]
    pub events: String,

    #[arg(long)]
    pub run_id: Option<String>,

    #[arg(long, default_value = "text")]
    pub format: String,

    #[arg(long, action = clap::ArgAction::Append)]
    pub set: Vec<String>,

    #[arg(long, default_value_t = false)]
    pub rerun_gatekeeper: bool,
}

#[derive(ClapArgs, Debug, Clone, Serialize, Deserialize)]
pub struct ResumeArgs {
    #[command(flatten)]
    pub run_args: RunArgs,

    #[arg(long)]
    pub run_id: String,
}

impl ResumeArgs {
    pub fn to_json_string(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn to_pretty_json_string(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json_str(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    pub fn to_json_value(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(self)
    }

    pub fn from_json_value(v: serde_json::Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(v)
    }
}

#[derive(ClapArgs, Debug, Clone)]
pub struct SearchArgs {
    /// Search query (required)
    #[arg(long)]
    pub query: String,

    /// Maximum number of results
    #[arg(long, default_value_t = 5)]
    pub limit: u32,

    /// Minimum relevance score threshold (0.0 - 1.0)
    #[arg(long, default_value_t = 0.6)]
    pub min_score: f32,

    /// Output format: json or markdown
    #[arg(long, default_value = "json")]
    pub format: String,

    /// Project ID (defaults to config)
    #[arg(long)]
    pub project_id: Option<String>,
}

#[derive(ClapArgs, Debug, Clone)]
pub struct RecordCandidateArgs {
    /// Original query/question (required)
    #[arg(long)]
    pub query: String,

    /// Answer/solution (required)
    #[arg(long)]
    pub answer: String,

    /// Comma-separated tags
    #[arg(long)]
    pub tags: Option<String>,

    /// Comma-separated file paths
    #[arg(long)]
    pub files: Option<String>,

    /// Additional metadata in JSON format
    #[arg(long)]
    pub metadata: Option<String>,

    /// Project ID (defaults to config)
    #[arg(long)]
    pub project_id: Option<String>,
}

#[derive(ClapArgs, Debug, Clone)]
pub struct RecordHitArgs {
    /// Comma-separated list of used QA IDs (required)
    #[arg(long)]
    pub qa_ids: String,

    /// Comma-separated list of shown QA IDs
    #[arg(long)]
    pub shown: Option<String>,

    /// Project ID (defaults to config)
    #[arg(long)]
    pub project_id: Option<String>,
}

#[derive(ClapArgs, Debug, Clone)]
pub struct RecordValidationArgs {
    /// QA ID to validate (required)
    #[arg(long)]
    pub qa_id: String,

    /// Whether the validation was successful
    #[arg(long)]
    pub success: bool,

    /// Confidence score (0.0 to 1.0)
    #[arg(long, default_value_t = 0.8)]
    pub confidence: f32,

    /// Project ID (defaults to config)
    #[arg(long)]
    pub project_id: Option<String>,
}

#[derive(ClapArgs, Debug, Clone)]
pub struct RecordSessionArgs {
    /// Session transcript file path (JSONL format)
    #[arg(long)]
    pub transcript: String,

    /// Session ID (internally treated as run_id for execution tracking)
    #[arg(long, alias = "run-id")]
    pub session_id: String,

    /// Project ID (defaults to config)
    #[arg(long)]
    pub project_id: Option<String>,

    /// Only extract knowledge, don't write to memory service
    #[arg(long, default_value_t = false)]
    pub extract_only: bool,
}

#[derive(ClapArgs, Debug, Clone)]
pub struct HttpServerArgs {
    /// Server port
    #[arg(long, default_value_t = 8080)]
    pub port: u16,

    /// Server host
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    /// Project ID (defaults to config)
    #[arg(long)]
    pub project_id: Option<String>,

    /// Session ID (defaults to generated UUID)
    #[arg(long)]
    pub session_id: Option<String>,
}

#[derive(ClapArgs, Debug, Clone)]
pub struct SyncStatusArgs {
    /// Output format: json or markdown
    #[arg(long, default_value = "markdown")]
    pub format: String,
}

#[derive(ClapArgs, Debug, Clone)]
pub struct SyncNowArgs {
    /// Wait for sync to complete before returning
    #[arg(long, default_value_t = false)]
    pub wait: bool,
}

#[derive(ClapArgs, Debug, Clone)]
pub struct SyncConflictsArgs {
    /// Output format: json or markdown
    #[arg(long, default_value = "markdown")]
    pub format: String,
}

#[derive(Subcommand, Debug, Clone)]
pub enum SyncCommand {
    /// Show current sync status
    Status(SyncStatusArgs),
    /// Trigger immediate synchronization
    Now(SyncNowArgs),
    /// List pending conflicts
    Conflicts(SyncConflictsArgs),
}

#[derive(ClapArgs, Debug, Clone)]
pub struct SyncArgs {
    #[command(subcommand)]
    pub command: SyncCommand,
}

#[derive(ClapArgs, Debug, Clone)]
pub struct DbInitArgs {
    /// Force reinitialize even if database exists
    #[arg(long, default_value_t = false)]
    pub force: bool,
}

#[derive(ClapArgs, Debug, Clone)]
pub struct DbInfoArgs {
    /// Output format: json or markdown
    #[arg(long, default_value = "markdown")]
    pub format: String,
}

#[derive(ClapArgs, Debug, Clone)]
pub struct DbExportArgs {
    /// Output file path (defaults to stdout)
    #[arg(long)]
    pub output: Option<String>,

    /// Export format: jsonl or csv
    #[arg(long, default_value = "jsonl")]
    pub format: String,

    /// Include validation records
    #[arg(long, default_value_t = false)]
    pub include_validations: bool,

    /// Include hit records
    #[arg(long, default_value_t = false)]
    pub include_hits: bool,
}

#[derive(ClapArgs, Debug, Clone)]
pub struct DbImportArgs {
    /// Input file path (required)
    #[arg(long)]
    pub input: String,

    /// Import format: jsonl or csv
    #[arg(long, default_value = "jsonl")]
    pub format: String,

    /// Skip existing items (by ID)
    #[arg(long, default_value_t = false)]
    pub skip_existing: bool,
}

#[derive(Subcommand, Debug, Clone)]
pub enum DbCommand {
    /// Initialize local database
    Init(DbInitArgs),
    /// Show database information
    Info(DbInfoArgs),
    /// Export database to file
    Export(DbExportArgs),
    /// Import data from file
    Import(DbImportArgs),
}

#[derive(ClapArgs, Debug, Clone)]
pub struct DbArgs {
    #[command(subcommand)]
    pub command: DbCommand,
}

#[derive(ClapArgs, Debug, Clone)]
pub struct InitArgs {
    /// Memory provider type: local, hybrid, or service
    #[arg(long, default_value = "local")]
    pub provider: String,

    /// Skip interactive prompts, use defaults
    #[arg(long, default_value_t = false)]
    pub non_interactive: bool,

    /// Ollama base URL (for local embeddings)
    #[arg(long, default_value = "http://localhost:11434")]
    pub ollama_url: String,

    /// OpenAI API key (for OpenAI embeddings)
    #[arg(long)]
    pub openai_key: Option<String>,

    /// Remote memory service URL (for hybrid mode)
    #[arg(long)]
    pub remote_url: Option<String>,

    /// Remote memory API key (for hybrid mode)
    #[arg(long)]
    pub remote_key: Option<String>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    Run(RunArgs),
    Replay(ReplayArgs),
    Resume(ResumeArgs),
    Search(SearchArgs),
    RecordCandidate(RecordCandidateArgs),
    RecordHit(RecordHitArgs),
    RecordValidation(RecordValidationArgs),
    RecordSession(RecordSessionArgs),
    HttpServer(HttpServerArgs),
    /// Initialize memex configuration
    Init(InitArgs),
    /// Memory synchronization commands
    Sync(SyncArgs),
    /// Local database management
    Db(DbArgs),
}
