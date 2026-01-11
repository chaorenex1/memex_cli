use thiserror::Error;

/// Shared executor error type (legacy path).
pub type ExecutorError = crate::error::ExecutorError;

/// Errors raised by task processors.
#[derive(Error, Debug)]
pub enum ProcessorError {
    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("io error: {0}")]
    Io(String),

    #[error("processor error: {0}")]
    Other(String),
}

impl From<std::io::Error> for ProcessorError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err.to_string())
    }
}
