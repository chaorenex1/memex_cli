use thiserror::Error;

/// 协议定义的错误代码（docs/STDIO_PROTOCOL.md 第3节）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ErrorCode {
    Success = 0,
    GeneralError = 1,
    ParseError = 2,
    ValidationError = 3,
    TaskNotFound = 10,
    DependencyError = 11,
    CircularDependency = 12,
    BackendError = 20,
    ModelNotFound = 21,
    QuotaExceeded = 22,
    Timeout = 30,
    Cancelled = 31,
    NetworkError = 40,
    AuthError = 41,
    ToolError = 50,
    PermissionDenied = 51,
    FileNotFound = 60,
    FileAccessDenied = 61,
    FileTooLarge = 62,
    TooManyFiles = 63,
    InvalidPath = 64,
    PathTraversal = 65,
    GlobNoMatch = 66,
    EncodingError = 67,
}

impl ErrorCode {
    pub fn as_u16(self) -> u16 {
        self as u16
    }
}

/// 协议化错误类型，覆盖解析/验证/执行等阶段
#[derive(Error, Debug)]
pub enum StdioError {
    #[error("no STDIO task blocks found")]
    NoTasks,

    #[error("metadata missing required field '{field}'")]
    MissingField { field: &'static str },

    #[error("metadata line is invalid: {0}")]
    InvalidMetadataLine(String),

    #[error("missing ---CONTENT--- marker")]
    MissingContentMarker,

    #[error("missing ---END--- marker")]
    MissingEndMarker,

    #[error("invalid task id: {0}")]
    InvalidId(String),

    #[error("duplicate task id: {0}")]
    DuplicateId(String),

    #[error("unknown dependency '{dep}' on task '{task}'")]
    UnknownDependency { task: String, dep: String },

    #[error("circular dependency detected")]
    CircularDependency,

    #[error("invalid number for {field}: {value}")]
    InvalidNumber { field: &'static str, value: String },

    #[error("file not found: {0}")]
    FileNotFound(String),

    #[error("file access denied: {0}")]
    FileAccessDenied(String),

    #[error("file too large: {0} bytes (limit: {1} bytes)")]
    FileTooLarge(u64, u64),

    #[error("too many files: {0} files (limit: {1})")]
    TooManyFiles(usize, usize),

    #[error("invalid file path: {0}")]
    InvalidPath(String),

    #[error("path traversal detected: {0}")]
    PathTraversal(String),

    #[error("glob pattern matched no files: {0}")]
    GlobNoMatch(String),

    #[error("file encoding error: {0}")]
    EncodingError(String),

    #[error("timeout after {0} seconds")]
    Timeout(u64),

    #[error("backend error: {0}")]
    BackendError(String),

    #[error("runner error: {0}")]
    RunnerError(String),
}

impl StdioError {
    pub fn error_code(&self) -> ErrorCode {
        match self {
            Self::NoTasks => ErrorCode::ParseError,
            Self::MissingField { .. } => ErrorCode::ParseError,
            Self::InvalidMetadataLine(_) => ErrorCode::ParseError,
            Self::MissingContentMarker => ErrorCode::ParseError,
            Self::MissingEndMarker => ErrorCode::ParseError,
            Self::InvalidId(_) => ErrorCode::ValidationError,
            Self::DuplicateId(_) => ErrorCode::ValidationError,
            Self::UnknownDependency { .. } => ErrorCode::DependencyError,
            Self::CircularDependency => ErrorCode::CircularDependency,
            Self::InvalidNumber { .. } => ErrorCode::ValidationError,
            Self::FileNotFound(_) => ErrorCode::FileNotFound,
            Self::FileAccessDenied(_) => ErrorCode::FileAccessDenied,
            Self::FileTooLarge(_, _) => ErrorCode::FileTooLarge,
            Self::TooManyFiles(_, _) => ErrorCode::TooManyFiles,
            Self::InvalidPath(_) => ErrorCode::InvalidPath,
            Self::PathTraversal(_) => ErrorCode::PathTraversal,
            Self::GlobNoMatch(_) => ErrorCode::GlobNoMatch,
            Self::EncodingError(_) => ErrorCode::EncodingError,
            Self::Timeout(_) => ErrorCode::Timeout,
            Self::BackendError(_) => ErrorCode::BackendError,
            Self::RunnerError(_) => ErrorCode::GeneralError,
        }
    }
}

/// 向后兼容别名
pub type StdioParseError = StdioError;
