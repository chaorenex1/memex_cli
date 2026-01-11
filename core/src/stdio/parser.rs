//! STDIO Protocol Parser (Compatibility Layer)
//!
//! This module provides backward-compatible public API for STDIO parsing.
//! The actual implementation has been moved to `parsers::standard`.
//!
//! **Migration Note**: New code should use `StandardStdioParser` trait directly
//! via `protocol::StdioProtocolParser` for better testability and flexibility.

use super::parsers::StandardStdioParser;
use super::protocol::StdioProtocolParser;
use super::types::StdioTask;
use crate::error::stdio::StdioError;

/// Parses STDIO tasks from input string
///
/// This is a backward-compatible wrapper around `StandardStdioParser`.
/// For new code, consider using `StandardStdioParser` directly through the trait.
///
/// # Performance
///
/// - Input < 10KB: Regular parser (simple, debuggable)
/// - Input >= 10KB: Zero-copy parser (~2x faster, lower memory)
///
/// # Errors
///
/// See `StdioError` for all possible error variants.
///
/// # Example
///
/// ```rust,ignore
/// use memex_core::stdio::parse_stdio_tasks;
///
/// let input = r#"
/// ---TASK---
/// id: example
/// backend: codex
/// workdir: /tmp
/// ---CONTENT---
/// echo "hello"
/// ---END---
/// "#;
///
/// let tasks = parse_stdio_tasks(input).unwrap();
/// assert_eq!(tasks.len(), 1);
/// ```
pub fn parse_stdio_tasks(input: &str) -> Result<Vec<StdioTask>, StdioError> {
    StandardStdioParser.parse_tasks(input)
}
