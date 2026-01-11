use thiserror::Error;

#[derive(Error, Debug)]
pub enum CliError {
    #[error("runner failed: {0}")]
    Runner(#[from] RunnerError),
    #[error("command failed: {0}")]
    Command(String),
    #[error("config error: {0}")]
    Config(String),
    #[error("replay failed: {0}")]
    Replay(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("anyhow error: {0}")]
    Anyhow(#[from] anyhow::Error),
}

#[derive(Error, Debug)]
pub enum RunnerError {
    #[error("config error: {0}")]
    Config(String),
    #[error("spawn failed: {0}")]
    Spawn(String),
    #[error("stream io error: {stream} {source}")]
    StreamIo {
        stream: &'static str,
        source: std::io::Error,
    },
    #[error("plugin error: {0}")]
    Plugin(#[from] anyhow::Error),
    #[error("stdio execution error: {0}")]
    Stdio(String),
}
