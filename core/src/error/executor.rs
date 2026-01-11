use thiserror::Error;

use super::stdio::{ErrorCode, StdioError};

/// Executor-specific errors for task graph construction and execution
#[derive(Error, Debug)]
pub enum ExecutorError {
    #[error("Duplicate task ID: {0}")]
    DuplicateTaskId(String),

    #[error("Dependency not found: task '{task_id}' depends on '{missing_dep}'")]
    DependencyNotFound {
        task_id: String,
        missing_dep: String,
    },

    #[error("Circular dependency detected: {0}")]
    CircularDependency(String),

    #[error("Task execution failed: {0}")]
    TaskExecutionFailed(String),

    #[error("Stage execution timeout")]
    StageTimeout,

    #[error("STDIO error: {0}")]
    Stdio(#[from] StdioError),

    #[error("Runner error: {0}")]
    Runner(String),
}

impl ExecutorError {
    /// Map executor error to protocol error code
    pub fn error_code(&self) -> ErrorCode {
        match self {
            Self::DuplicateTaskId(_) => ErrorCode::ValidationError,
            Self::DependencyNotFound { .. } => ErrorCode::DependencyError,
            Self::CircularDependency(_) => ErrorCode::CircularDependency,
            Self::TaskExecutionFailed(_) => ErrorCode::GeneralError,
            Self::StageTimeout => ErrorCode::Timeout,
            Self::Stdio(e) => e.error_code(),
            Self::Runner(_) => ErrorCode::GeneralError,
        }
    }
}
