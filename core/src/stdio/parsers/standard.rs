//! Standard STDIO Protocol Parser
//!
//! Implements the standard STDIO protocol as defined in docs/STDIO_PROTOCOL.md.
//! Supports both regular and zero-copy parsing for performance optimization.
//!
//! # Format
//!
//! ```text
//! ---TASK---
//! id: task1
//! backend: codex
//! workdir: /path
//! ---CONTENT---
//! Task content here
//! ---END---
//! ```

use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use crate::error::stdio::StdioError;
use crate::stdio::id_gen::generate_task_id;
use crate::stdio::protocol::{FormatError, FormatValidation, StdioProtocolParser};
use crate::stdio::types::{FilesEncoding, FilesMode, StdioTask};

/// Standard STDIO protocol parser
///
/// This is the default parser for memex-cli. It implements the STDIO protocol
/// with optimizations for both small (<10KB) and large inputs (zero-copy parsing).
#[derive(Debug, Clone, Copy)]
pub struct StandardStdioParser;

impl StdioProtocolParser for StandardStdioParser {
    fn name(&self) -> &str {
        "standard"
    }

    fn parse_tasks(&self, input: &str) -> Result<Vec<StdioTask>, StdioError> {
        parse_stdio_tasks_internal(input)
    }

    fn validate_format(&self, input: &str) -> FormatValidation {
        // Quick validation without full parsing
        if !input.contains("---TASK---") {
            return FormatValidation::with_errors(vec![FormatError::parse_error(
                None,
                "No '---TASK---' marker found. Did you forget to use --no-structured-text for plain prompts?".to_string(),
            )]);
        }

        if !input.contains("---CONTENT---") {
            return FormatValidation::with_errors(vec![FormatError::parse_error(
                None,
                "No '---CONTENT---' marker found".to_string(),
            )]);
        }

        if !input.contains("---END---") {
            return FormatValidation::with_errors(vec![FormatError::parse_error(
                None,
                "No '---END---' marker found".to_string(),
            )]);
        }

        // Try full parse to detect detailed errors
        match self.parse_tasks(input) {
            Ok(_) => FormatValidation::valid(),
            Err(e) => {
                FormatValidation::with_errors(vec![FormatError::parse_error(None, e.to_string())])
            }
        }
    }

    fn format_identifier(&self) -> &str {
        "---TASK---"
    }
}

// ============================================================================
// Public parsing function (backward compatibility)
// ============================================================================

/// Parses STDIO tasks from input string
///
/// This function is the main entry point for parsing STDIO protocol input.
/// It automatically selects between regular and zero-copy parsing based on input size.
///
/// # Performance
///
/// - Input < 10KB: Regular parser (simple, debuggable)
/// - Input >= 10KB: Zero-copy parser (~2x faster, lower memory)
///
/// # Errors
///
/// See `StdioError` for all possible error variants.
#[allow(clippy::while_let_on_iterator)]
pub fn parse_stdio_tasks_internal(input: &str) -> Result<Vec<StdioTask>, StdioError> {
    // Level 2.3: Smart parser selection (zero-copy vs original)
    const ZERO_COPY_THRESHOLD: usize = 10 * 1024; // 10KB

    if input.len() >= ZERO_COPY_THRESHOLD {
        // Large input: use zero-copy version (2x speedup)
        return parse_stdio_tasks_zero_copy(input);
    }

    // Small input: use original (simpler, debug-friendly)
    let mut lines = input.lines().peekable();
    let mut tasks: Vec<StdioTask> = Vec::new();

    while let Some(line) = lines.next() {
        if line.trim() != "---TASK---" {
            continue;
        }

        let mut metadata: HashMap<String, String> = HashMap::new();
        let mut saw_content_marker = false;

        while let Some(line) = lines.next() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed == "---CONTENT---" {
                saw_content_marker = true;
                break;
            }
            let Some((k, v)) = trimmed.split_once(':') else {
                return Err(StdioError::InvalidMetadataLine(trimmed.to_string()));
            };
            metadata.insert(k.trim().to_lowercase(), v.trim().to_string());
        }

        if !saw_content_marker {
            return Err(StdioError::MissingContentMarker);
        }

        let mut content_lines: Vec<String> = Vec::new();
        let mut ended = false;
        while let Some(line) = lines.next() {
            if line.trim() == "---END---" {
                ended = true;
                break;
            }
            content_lines.push(line.to_string());
        }

        if !ended {
            return Err(StdioError::MissingEndMarker);
        }

        let id = metadata.get("id").cloned().unwrap_or_else(generate_task_id);
        let backend = metadata
            .get("backend")
            .cloned()
            .ok_or(StdioError::MissingField { field: "backend" })?;
        let workdir = metadata
            .get("workdir")
            .cloned()
            .ok_or(StdioError::MissingField { field: "workdir" })?;

        validate_id(&id)?;

        let dependencies = metadata
            .get("dependencies")
            .map(|s| split_csv(s))
            .unwrap_or_default();
        let stream_format = metadata
            .get("stream-format")
            .cloned()
            .unwrap_or_else(|| "text".to_string());
        let model = metadata.get("model").cloned();
        let model_provider = metadata.get("model-provider").cloned();
        let timeout = parse_u64(metadata.get("timeout").map(String::as_str), "timeout")?;
        let retry = parse_u32(metadata.get("retry").map(String::as_str), "retry")?;
        let files = metadata
            .get("files")
            .map(|s| split_csv(s))
            .unwrap_or_default();
        let files_mode = parse_files_mode(metadata.get("files-mode"));
        let files_encoding = parse_files_encoding(metadata.get("files-encoding"));

        let content = content_lines.join("\n");

        tasks.push(StdioTask {
            id,
            backend,
            workdir,
            model,
            model_provider,
            dependencies,
            stream_format,
            timeout,
            retry,
            files,
            files_mode,
            files_encoding,
            content,
            backend_kind: None,
            env_file: None,
            env: None,
            task_level: None,
            resume_run_id: None,
            resume_context: None,
        });
    }

    if tasks.is_empty() {
        return Err(StdioError::NoTasks);
    }

    validate_dependencies(&tasks)?;
    Ok(tasks)
}

// ============================================================================
// Zero-Copy Parser (Level 2.3 optimization)
// ============================================================================

/// Zero-copy parser: uses string slices to avoid intermediate allocations
///
/// # Advantages
///
/// - Avoids per-line String allocation (uses `&str` slices)
/// - Reduces intermediate Vec<String> allocations
/// - ~2x performance improvement for large inputs (>10KB)
pub fn parse_stdio_tasks_zero_copy(input: &str) -> Result<Vec<StdioTask>, StdioError> {
    let mut tasks: Vec<StdioTask> = Vec::new();
    let mut pos = 0;

    while let Some(task_start) = input[pos..].find("---TASK---") {
        pos += task_start + 10; // "---TASK---".len()

        // Find CONTENT marker
        let Some(content_start) = input[pos..].find("---CONTENT---") else {
            return Err(StdioError::MissingContentMarker);
        };

        // Metadata section (using slice, no copy)
        let metadata_section = &input[pos..pos + content_start];
        let metadata = parse_metadata_zero_copy(metadata_section)?;

        pos += content_start + 13; // "---CONTENT---".len()

        // Find END marker
        let Some(end_pos) = input[pos..].find("---END---") else {
            return Err(StdioError::MissingEndMarker);
        };

        // Content section (slice)
        let content = &input[pos..pos + end_pos];

        // Build task (only here we convert to String)
        tasks.push(build_task_from_metadata_zero_copy(metadata, content)?);

        pos += end_pos + 9; // "---END---".len()
    }

    if tasks.is_empty() {
        return Err(StdioError::NoTasks);
    }

    validate_dependencies(&tasks)?;
    Ok(tasks)
}

/// Parse metadata section (zero-copy: returns &str references)
fn parse_metadata_zero_copy(section: &str) -> Result<HashMap<&str, &str>, StdioError> {
    let mut metadata = HashMap::new();

    for line in section.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let Some((k, v)) = trimmed.split_once(':') else {
            return Err(StdioError::InvalidMetadataLine(trimmed.to_string()));
        };

        metadata.insert(k.trim(), v.trim());
    }

    Ok(metadata)
}

/// Build task from zero-copy metadata (only allocates String here)
fn build_task_from_metadata_zero_copy(
    metadata: HashMap<&str, &str>,
    content: &str,
) -> Result<StdioTask, StdioError> {
    // Required fields
    let id = metadata
        .get("id")
        .map(|s| s.to_string())
        .unwrap_or_else(generate_task_id);

    validate_id(&id)?;

    let backend = metadata
        .get("backend")
        .ok_or(StdioError::MissingField { field: "backend" })?
        .to_string();

    let workdir = metadata
        .get("workdir")
        .ok_or(StdioError::MissingField { field: "workdir" })?
        .to_string();

    // Optional fields
    let dependencies = metadata
        .get("dependencies")
        .map(|s| split_csv_zero_copy(s))
        .unwrap_or_default();

    let stream_format = metadata
        .get("stream-format")
        .map(|s| s.to_string())
        .unwrap_or_else(|| "text".to_string());

    let model = metadata.get("model").map(|s| s.to_string());
    let model_provider = metadata.get("model-provider").map(|s| s.to_string());

    let timeout = parse_u64_zero_copy(metadata.get("timeout").copied(), "timeout")?;
    let retry = parse_u32_zero_copy(metadata.get("retry").copied(), "retry")?;

    let files = metadata
        .get("files")
        .map(|s| split_csv_zero_copy(s))
        .unwrap_or_default();

    let files_mode = parse_files_mode_zero_copy(metadata.get("files-mode").copied());
    let files_encoding = parse_files_encoding_zero_copy(metadata.get("files-encoding").copied());

    let content = strip_trailing_newline(content);

    Ok(StdioTask {
        id,
        backend,
        workdir,
        model,
        model_provider,
        dependencies,
        stream_format,
        timeout,
        retry,
        files,
        files_mode,
        files_encoding,
        content: content.to_string(),
        backend_kind: None,
        env_file: None,
        env: None,
        task_level: None,
        resume_run_id: None,
        resume_context: None,
    })
}

/// CSV split (zero-copy version)
fn split_csv_zero_copy(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

/// Parse u64 (zero-copy version)
fn parse_u64_zero_copy(
    value: Option<&str>,
    field: &'static str,
) -> Result<Option<u64>, StdioError> {
    match value {
        None => Ok(None),
        Some(v) if v.trim().is_empty() => Ok(None),
        Some(v) => v
            .trim()
            .parse::<u64>()
            .map(Some)
            .map_err(|_| StdioError::InvalidNumber {
                field,
                value: v.to_string(),
            }),
    }
}

/// Parse u32 (zero-copy version)
fn parse_u32_zero_copy(
    value: Option<&str>,
    field: &'static str,
) -> Result<Option<u32>, StdioError> {
    match value {
        None => Ok(None),
        Some(v) if v.trim().is_empty() => Ok(None),
        Some(v) => v
            .trim()
            .parse::<u32>()
            .map(Some)
            .map_err(|_| StdioError::InvalidNumber {
                field,
                value: v.to_string(),
            }),
    }
}

/// Parse files mode (zero-copy version)
fn parse_files_mode_zero_copy(v: Option<&str>) -> FilesMode {
    match v.map(|s| s.to_lowercase()) {
        Some(ref s) if s == "embed" => FilesMode::Embed,
        Some(ref s) if s == "ref" => FilesMode::Ref,
        _ => FilesMode::Auto,
    }
}

/// Parse files encoding (zero-copy version)
fn parse_files_encoding_zero_copy(v: Option<&str>) -> FilesEncoding {
    match v.map(|s| s.to_lowercase()) {
        Some(ref s) if s == "utf-8" || s == "utf8" => FilesEncoding::Utf8,
        Some(ref s) if s == "base64" => FilesEncoding::Base64,
        _ => FilesEncoding::Auto,
    }
}

// ============================================================================
// Original Parser Helpers
// ============================================================================

fn split_csv(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn parse_u64(value: Option<&str>, field: &'static str) -> Result<Option<u64>, StdioError> {
    match value {
        None => Ok(None),
        Some(v) if v.trim().is_empty() => Ok(None),
        Some(v) => v
            .trim()
            .parse::<u64>()
            .map(Some)
            .map_err(|_| StdioError::InvalidNumber {
                field,
                value: v.to_string(),
            }),
    }
}

fn parse_u32(value: Option<&str>, field: &'static str) -> Result<Option<u32>, StdioError> {
    match value {
        None => Ok(None),
        Some(v) if v.trim().is_empty() => Ok(None),
        Some(v) => v
            .trim()
            .parse::<u32>()
            .map(Some)
            .map_err(|_| StdioError::InvalidNumber {
                field,
                value: v.to_string(),
            }),
    }
}

fn parse_files_mode(v: Option<&String>) -> FilesMode {
    match v.map(|s| s.to_lowercase()) {
        Some(ref s) if s == "embed" => FilesMode::Embed,
        Some(ref s) if s == "ref" => FilesMode::Ref,
        _ => FilesMode::Auto,
    }
}

fn parse_files_encoding(v: Option<&String>) -> FilesEncoding {
    match v.map(|s| s.to_lowercase()) {
        Some(ref s) if s == "utf-8" || s == "utf8" => FilesEncoding::Utf8,
        Some(ref s) if s == "base64" => FilesEncoding::Base64,
        _ => FilesEncoding::Auto,
    }
}

fn validate_id(id: &str) -> Result<(), StdioError> {
    static RESERVED: &[&str] = &[
        "_root", "_start", "_end", "_all", "_none", "_self", "_parent",
    ];
    if RESERVED.contains(&id) || id.starts_with("__") {
        return Err(StdioError::InvalidId(id.to_string()));
    }
    static ID_REGEX: OnceLock<Regex> = OnceLock::new();
    let re = ID_REGEX.get_or_init(|| Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_\-\.]{0,127}$").unwrap());
    if !re.is_match(id) {
        return Err(StdioError::InvalidId(id.to_string()));
    }
    Ok(())
}

fn strip_trailing_newline(input: &str) -> &str {
    if let Some(stripped) = input.strip_suffix("\r\n") {
        stripped
    } else if let Some(stripped) = input.strip_suffix('\n') {
        stripped
    } else {
        input
    }
}

fn validate_dependencies(tasks: &[StdioTask]) -> Result<(), StdioError> {
    let mut ids: HashSet<&str> = HashSet::new();
    for t in tasks {
        if !ids.insert(&t.id) {
            return Err(StdioError::DuplicateId(t.id.clone()));
        }
    }
    for t in tasks {
        for dep in &t.dependencies {
            if !ids.contains(dep.as_str()) {
                return Err(StdioError::UnknownDependency {
                    task: t.id.clone(),
                    dep: dep.clone(),
                });
            }
        }
    }

    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    let lookup: HashMap<&str, &StdioTask> = tasks.iter().map(|t| (t.id.as_str(), t)).collect();

    fn dfs<'a>(
        id: &'a str,
        lookup: &HashMap<&'a str, &'a StdioTask>,
        visiting: &mut HashSet<&'a str>,
        visited: &mut HashSet<&'a str>,
    ) -> bool {
        if visited.contains(id) {
            return false;
        }
        if !visiting.insert(id) {
            return true;
        }
        if let Some(task) = lookup.get(id) {
            for dep in &task.dependencies {
                if dfs(dep, lookup, visiting, visited) {
                    return true;
                }
            }
        }
        visiting.remove(id);
        visited.insert(id);
        false
    }

    for id in ids {
        if dfs(id, &lookup, &mut visiting, &mut visited) {
            return Err(StdioError::CircularDependency);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_task_preserves_multiline_content() {
        let input = r#"
---TASK---
id: t1
backend: codex
workdir: .
---CONTENT---
line1
line2
---END---
"#;
        let tasks = parse_stdio_tasks_internal(input).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].content, "line1\nline2");
        assert_eq!(tasks[0].stream_format, "text");
    }

    #[test]
    fn parse_generates_id_when_missing() {
        let input = r#"
---TASK---
backend: codex
workdir: .
---CONTENT---
hello
---END---
"#;
        let tasks = parse_stdio_tasks_internal(input).unwrap();
        assert_eq!(tasks.len(), 1);
        assert!(!tasks[0].id.trim().is_empty());
        assert!(tasks[0].id.starts_with("task-"));
    }

    #[test]
    fn parse_validates_unknown_dependency() {
        let input = r#"
---TASK---
id: a
backend: codex
workdir: .
dependencies: b
---CONTENT---
hello
---END---
"#;
        let err = parse_stdio_tasks_internal(input).unwrap_err();
        assert!(matches!(err, StdioError::UnknownDependency { .. }));
    }

    #[test]
    fn parse_detects_cycle() {
        let input = r#"
---TASK---
id: a
backend: codex
workdir: .
dependencies: b
---CONTENT---
a
---END---

---TASK---
id: b
backend: codex
workdir: .
dependencies: a
---CONTENT---
b
---END---
"#;
        let err = parse_stdio_tasks_internal(input).unwrap_err();
        assert!(matches!(err, StdioError::CircularDependency));
    }

    #[test]
    fn trait_implementation() {
        let parser = StandardStdioParser;
        assert_eq!(parser.name(), "standard");
        assert_eq!(parser.format_identifier(), "---TASK---");
    }

    #[test]
    fn validate_format_missing_marker() {
        let parser = StandardStdioParser;
        let validation = parser.validate_format("just plain text");
        assert!(!validation.is_valid);
        assert_eq!(validation.errors.len(), 1);
        assert!(validation.errors[0].message.contains("---TASK---"));
    }

    #[test]
    fn validate_format_valid() {
        let parser = StandardStdioParser;
        let input = r#"
---TASK---
id: test
backend: codex
workdir: .
---CONTENT---
hello
---END---
"#;
        let validation = parser.validate_format(input);
        assert!(validation.is_valid);
        assert!(validation.errors.is_empty());
    }
}
