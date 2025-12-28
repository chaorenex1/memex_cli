use clap::{Args as ClapArgs, Parser, Subcommand};

#[derive(clap::ValueEnum, Debug, Clone, Copy)]
pub enum BackendKind {
    Auto,
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

#[derive(Parser, Debug)]
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
    #[arg(long, value_enum, default_value_t = BackendKind::Auto)]
    pub backend_kind: BackendKind,

    #[arg(long)]
    pub model: Option<String>,

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

    #[arg(long)]
    pub stream: bool,

    #[arg(long, default_value = "text")]
    pub stream_format: String,

    /// Extra environment variables to pass to the backend process (KEY=VALUE).
    /// Can be specified multiple times.
    #[arg(long = "env", action = clap::ArgAction::Append)]
    pub env: Vec<String>,

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

#[derive(Subcommand, Debug)]
pub enum Commands {
    Run(RunArgs),
    Replay(ReplayArgs),
    Resume(ResumeArgs),
}
