use clap::{Args as ClapArgs, Parser, Subcommand};

#[derive(clap::ValueEnum, Debug, Clone, Copy)]
pub enum BackendKind {
    Codecli,
    Aiservice,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskLevel {
    Auto,
    L0,
    L1,
    L2,
    L3,
}

#[derive(Parser, Debug, Clone)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[arg(long, default_value = "codex", global = true)]
    pub codecli_bin: String,

    #[arg(trailing_var_arg = true, global = true)]
    pub codecli_args: Vec<String>,

    #[arg(long, default_value_t = 65536, global = true)]
    pub capture_bytes: usize,
}

#[derive(ClapArgs, Debug, Clone)]
pub struct RunArgs {
    #[arg(long)]
    pub backend: String,

    /// Explicitly select how to interpret `--backend`.
    /// - auto: URL => aiservice, otherwise => codecli
    /// - codecli: treat backend as a local binary name/path
    /// - aiservice: treat backend as an http(s) URL
    #[arg(long, value_enum)]
    pub backend_kind: Option<BackendKind>,

    #[arg(long)]
    pub model: Option<String>,

    #[arg(long)]
    pub model_provider: Option<String>,

    /// Task level for scheduling/strategy hints.
    /// - auto: infer from prompt (fast heuristic)
    /// - L0..L3: explicitly set
    #[arg(long, value_enum, default_value_t = TaskLevel::Auto)]
    pub task_level: TaskLevel,

    #[arg(long, group = "input")]
    pub prompt: Option<String>,

    #[arg(long, group = "input")]
    pub prompt_file: Option<String>,

    #[arg(long, group = "input")]
    pub stdin: bool,

    #[arg(long, default_value = "text")]
    pub stream_format: String,

    /// Force TUI mode (does not affect `--stream-format`).
    #[arg(long, default_value_t = false)]
    pub tui: bool,

    /// Extra environment variables to pass to the backend process (KEY=VALUE).
    /// Can be specified multiple times.
    #[arg(long = "env", action = clap::ArgAction::Append)]
    pub env: Vec<String>,

    /// Load environment variables from a file (KEY=VALUE per line).
    /// Lines starting with # are ignored. Empty lines are not allowed.
    #[arg(long = "env-file", alias = "env_file")]
    pub env_file: Option<String>,

    #[arg(long)]
    pub project_id: Option<String>,

    #[arg(long)]
    pub memory_base_url: Option<String>,

    #[arg(long)]
    pub memory_api_key: Option<String>,
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

#[derive(ClapArgs, Debug, Clone)]
pub struct ResumeArgs {
    #[command(flatten)]
    pub run_args: RunArgs,

    #[arg(long)]
    pub run_id: String,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    Run(RunArgs),
    Replay(ReplayArgs),
    Resume(ResumeArgs),
}
