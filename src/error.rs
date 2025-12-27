use thiserror::Error;

#[derive(Error, Debug)]
pub enum CliError {
    #[error("runner error")]
    Runner(#[from] RunnerError),
}

#[derive(Error, Debug)]
pub enum RunnerError {
    #[error("spawn failed: {0}")]
    Spawn(String),

    #[error("io error on {stream}")]
    StreamIo {
        stream: &'static str,
        #[source]
        source: std::io::Error,
    },
}
