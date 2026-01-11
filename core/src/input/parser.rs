//! Input Parser
//!
//! Unified input processing that supports both structured (STDIO protocol)
//! and plain text modes.

use crate::stdio::{
    generate_task_id, FilesEncoding, FilesMode, StandardStdioParser, StdioProtocolParser, StdioTask,
};

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
    pub fn parse(
        input: &str,
        structured: bool,
        default_backend: &str,
        default_workdir: &str,
        default_model: Option<String>,
        default_stream_format: &str,
    ) -> Result<Vec<StdioTask>, String> {
        if structured {
            // Structured mode: parse as STDIO protocol
            let parser = StandardStdioParser;
            parser.parse_tasks(input).map_err(|e| {
                format!(
                    "Failed to parse structured text: {}\n\n💡 Tip: Use --no-structured-text to treat input as plain text",
                    e
                )
            })
        } else {
            // Plain text mode: wrap as single task
            Ok(vec![wrap_as_plain_text_task(
                input,
                default_backend,
                default_workdir,
                default_model,
                default_stream_format,
            )])
        }
    }
}

/// Wraps plain text content as a single STDIO task
///
/// Generates an auto-generated task ID with timestamp and random suffix.
/// All other fields use provided defaults or sensible defaults.
///
/// # Arguments
///
/// * `content` - The plain text content to wrap
/// * `backend` - Backend name (e.g., "codex", "claude")
/// * `workdir` - Working directory path
/// * `model` - Optional model name
/// * `stream_format` - Stream format (text/jsonl)
///
/// # Returns
///
/// A single `StdioTask` with auto-generated ID
fn wrap_as_plain_text_task(
    content: &str,
    backend: &str,
    workdir: &str,
    model: Option<String>,
    stream_format: &str,
) -> StdioTask {
    StdioTask {
        id: generate_task_id(),
        backend: backend.to_string(),
        workdir: workdir.to_string(),
        model,
        model_provider: None,
        dependencies: vec![],
        stream_format: stream_format.to_string(),
        timeout: None,
        retry: None,
        files: vec![],
        files_mode: FilesMode::Auto,
        files_encoding: FilesEncoding::Auto,
        content: content.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text_mode() {
        let input = "echo hello world";
        let tasks = InputParser::parse(
            input, false, // structured = false
            "codex", "/project", None, "text",
        )
        .unwrap();

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].content, "echo hello world");
        assert_eq!(tasks[0].backend, "codex");
        assert_eq!(tasks[0].workdir, "/project");
        assert_eq!(tasks[0].stream_format, "text");
        assert!(tasks[0].id.starts_with("task-"));
        assert!(tasks[0].dependencies.is_empty());
    }

    #[test]
    fn test_plain_text_with_model() {
        let tasks = InputParser::parse(
            "test prompt",
            false,
            "claude",
            "/tmp",
            Some("claude-sonnet-4".to_string()),
            "jsonl",
        )
        .unwrap();

        assert_eq!(tasks[0].backend, "claude");
        assert_eq!(tasks[0].model, Some("claude-sonnet-4".to_string()));
        assert_eq!(tasks[0].stream_format, "jsonl");
    }

    #[test]
    fn test_structured_mode_success() {
        let input = r#"
---TASK---
id: test
backend: codex
workdir: /project
---CONTENT---
echo hello
---END---
"#;

        let tasks = InputParser::parse(
            input,
            true, // structured = true
            "default_backend",
            "/default",
            None,
            "text",
        )
        .unwrap();

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "test");
        assert_eq!(tasks[0].backend, "codex"); // From input, not default
        assert_eq!(tasks[0].content.trim(), "echo hello");
    }

    #[test]
    fn test_structured_mode_error_helpful() {
        let input = "just plain text without markers";
        let result = InputParser::parse(input, true, "codex", "/tmp", None, "text");

        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        assert!(err_msg.contains("Failed to parse structured text"));
        assert!(err_msg.contains("--no-structured-text"));
    }

    #[test]
    fn test_wrap_as_plain_text_task() {
        let task = wrap_as_plain_text_task("test content", "codex", "/work", None, "text");

        assert_eq!(task.content, "test content");
        assert_eq!(task.backend, "codex");
        assert_eq!(task.workdir, "/work");
        assert!(task.id.starts_with("task-"));
        assert_eq!(task.files_mode, FilesMode::Auto);
        assert_eq!(task.files_encoding, FilesEncoding::Auto);
        assert!(task.files.is_empty());
    }

    #[test]
    fn test_auto_task_id_uniqueness() {
        let task1 = wrap_as_plain_text_task("test1", "codex", "/tmp", None, "text");
        let task2 = wrap_as_plain_text_task("test2", "codex", "/tmp", None, "text");

        // IDs should be different (due to random suffix)
        assert_ne!(task1.id, task2.id);
    }
}
