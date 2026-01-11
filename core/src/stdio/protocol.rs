//! STDIO Protocol Parser Trait
//!
//! Defines the abstraction for parsing STDIO protocol input into tasks.
//! This trait allows pluggable parser implementations (standard, YAML, TOML variants).
//!
//! # Design
//!
//! - `StdioProtocolParser`: Core trait for parsing and validation
//! - `FormatValidation`: Rich validation results with warnings/errors
//! - Future-proof for alternative format parsers

use crate::error::stdio::StdioError;
use crate::stdio::types::StdioTask;

/// Stdio protocol parser trait
///
/// Implementations must be thread-safe (Send + Sync) to support concurrent parsing.
pub trait StdioProtocolParser: Send + Sync {
    /// Returns the parser name (e.g., "standard", "yaml-variant")
    fn name(&self) -> &str;

    /// Parses input string into a list of tasks
    ///
    /// # Errors
    ///
    /// Returns `StdioError` if:
    /// - Input is missing required markers (---TASK---, ---CONTENT---, ---END---)
    /// - Metadata fields are invalid or missing
    /// - Task IDs are duplicate or invalid
    /// - Dependencies form cycles or reference non-existent tasks
    fn parse_tasks(&self, input: &str) -> Result<Vec<StdioTask>, StdioError>;

    /// Validates input format without full parsing
    ///
    /// Useful for early validation or providing helpful error messages.
    /// Returns detailed warnings and errors with line numbers.
    fn validate_format(&self, input: &str) -> FormatValidation;

    /// Returns a format identifier for auto-detection
    ///
    /// Example: "---TASK---" for standard STDIO protocol
    fn format_identifier(&self) -> &str;
}

/// Format validation result
///
/// Contains both hard errors (prevents parsing) and soft warnings (style issues).
#[derive(Debug, Clone)]
pub struct FormatValidation {
    /// Whether the format is valid enough to attempt parsing
    pub is_valid: bool,

    /// Non-fatal warnings (e.g., missing optional fields, style issues)
    pub warnings: Vec<FormatWarning>,

    /// Fatal errors that prevent parsing
    pub errors: Vec<FormatError>,
}

impl FormatValidation {
    /// Creates a validation result with only errors
    pub fn with_errors(errors: Vec<FormatError>) -> Self {
        Self {
            is_valid: false,
            warnings: vec![],
            errors,
        }
    }

    /// Creates a successful validation result
    pub fn valid() -> Self {
        Self {
            is_valid: true,
            warnings: vec![],
            errors: vec![],
        }
    }

    /// Creates a validation result with warnings but no errors
    pub fn with_warnings(warnings: Vec<FormatWarning>) -> Self {
        Self {
            is_valid: true,
            warnings,
            errors: vec![],
        }
    }
}

/// Format warning (non-fatal)
#[derive(Debug, Clone)]
pub struct FormatWarning {
    /// Line number where the warning occurred (1-indexed, None if global)
    pub line: Option<usize>,

    /// Warning message
    pub message: String,

    /// Optional suggestion for fixing the warning
    pub suggestion: Option<String>,
}

impl FormatWarning {
    /// Creates a new warning with optional suggestion
    pub fn new(line: Option<usize>, message: String, suggestion: Option<String>) -> Self {
        Self {
            line,
            message,
            suggestion,
        }
    }
}

/// Format error (fatal, prevents parsing)
#[derive(Debug, Clone)]
pub struct FormatError {
    /// Line number where the error occurred (1-indexed, None if global)
    pub line: Option<usize>,

    /// Error code from STDIO protocol specification
    pub code: u16,

    /// Error message
    pub message: String,
}

impl FormatError {
    /// Creates a new format error
    pub fn new(line: Option<usize>, code: u16, message: String) -> Self {
        Self {
            line,
            code,
            message,
        }
    }

    /// Creates a parse error (code 2)
    pub fn parse_error(line: Option<usize>, message: String) -> Self {
        Self::new(line, 2, message)
    }

    /// Creates a validation error (code 3)
    pub fn validation_error(line: Option<usize>, message: String) -> Self {
        Self::new(line, 3, message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_validation_constructors() {
        let valid = FormatValidation::valid();
        assert!(valid.is_valid);
        assert!(valid.errors.is_empty());
        assert!(valid.warnings.is_empty());

        let with_warnings = FormatValidation::with_warnings(vec![FormatWarning::new(
            Some(10),
            "Style issue".to_string(),
            Some("Fix it".to_string()),
        )]);
        assert!(with_warnings.is_valid);
        assert_eq!(with_warnings.warnings.len(), 1);

        let with_errors = FormatValidation::with_errors(vec![FormatError::parse_error(
            Some(5),
            "Missing marker".to_string(),
        )]);
        assert!(!with_errors.is_valid);
        assert_eq!(with_errors.errors.len(), 1);
        assert_eq!(with_errors.errors[0].code, 2);
    }

    #[test]
    fn format_error_constructors() {
        let parse_err = FormatError::parse_error(Some(10), "Parse failed".to_string());
        assert_eq!(parse_err.code, 2);
        assert_eq!(parse_err.line, Some(10));

        let validation_err = FormatError::validation_error(None, "Invalid ID".to_string());
        assert_eq!(validation_err.code, 3);
        assert_eq!(validation_err.line, None);
    }
}
