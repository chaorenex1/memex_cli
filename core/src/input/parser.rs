//! Input Parser
//!
//! Unified input processing that supports both structured (STDIO protocol)
//! and plain text modes.

use crate::stdio::{StandardStdioParser, StdioProtocolParser, StdioTask};

/// Input parser for memex-cli
///
/// Provides a unified interface for parsing user input into `StdioTask` lists.
/// Supports two modes:
/// - **Structured mode**: Parses STDIO protocol format (multi-task with dependencies)
/// - **Plain text mode**: Wraps input as a single task
pub struct InputParser;

impl InputParser {
    /// Parses input into a list of tasks
    ///
    /// # Arguments
    ///
    /// * `input` - The raw input string (from --prompt, --prompt-file, or --stdin)
    /// * `structured` - Whether to parse as structured STDIO protocol
    /// * `default_backend` - Backend to use for plain text mode
    /// * `default_workdir` - Working directory for plain text mode
    /// * `default_model` - Optional model name for plain text mode
    /// * `default_stream_format` - Stream format (text/jsonl) for plain text mode
    ///
    /// # Returns
    ///
    /// - `Ok(Vec<StdioTask>)`: Parsed task list
    /// - `Err(String)`: User-friendly error message with suggestions
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// // Plain text mode
    /// let tasks = InputParser::parse(
    ///     "echo hello",
    ///     false, // structured = false
    ///     "codex",
    ///     "/tmp",
    ///     None,
    ///     "text",
    /// ).unwrap();
    /// assert_eq!(tasks.len(), 1);
    ///
    /// // Structured mode
    /// let input = r#"
    /// ---TASK---
    /// id: test
    /// backend: codex
    /// workdir: /tmp
    /// ---CONTENT---
    /// echo hello
    /// ---END---
    /// "#;
    /// let tasks = InputParser::parse(input, true, "codex", "/tmp", None, "text").unwrap();
    /// assert_eq!(tasks.len(), 1);
    /// ```
    pub fn parse(input: &str, structured: bool) -> Result<Vec<StdioTask>, String> {
        if structured {
            // Structured mode: parse as STDIO protocol
            let parser = StandardStdioParser;
            let tasks = parser.parse_tasks(input).unwrap_or_default();
            if tasks.is_empty() {
                tracing::error!("Failed to parse structured text input: no tasks found");
            }
            Ok(tasks)
        } else {
            // Plain text mode: wrap as single task
            Ok(vec![])
        }
    }
}
