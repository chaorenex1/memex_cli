use std::path::Path;

use crate::error::stdio::StdioError;

use super::{StdioRunOpts, StdioTask};

fn io_error_to_stdio_error(path: &Path, err: std::io::Error) -> StdioError {
    match err.kind() {
        std::io::ErrorKind::NotFound => StdioError::FileNotFound(path.display().to_string()),
        std::io::ErrorKind::PermissionDenied => {
            StdioError::FileAccessDenied(path.display().to_string())
        }
        _ => StdioError::RunnerError(format!("io error on {}: {}", path.display(), err)),
    }
}

fn serde_error_to_stdio_error(context: &'static str, err: serde_json::Error) -> StdioError {
    StdioError::RunnerError(format!("serde_json {context}: {err}"))
}

pub fn stdio_task_to_json(task: &StdioTask) -> Result<String, StdioError> {
    serde_json::to_string(task).map_err(|e| serde_error_to_stdio_error("serialize StdioTask", e))
}

pub fn stdio_task_to_pretty_json(task: &StdioTask) -> Result<String, StdioError> {
    serde_json::to_string_pretty(task)
        .map_err(|e| serde_error_to_stdio_error("serialize StdioTask", e))
}

pub fn stdio_task_from_json(json: &str) -> Result<StdioTask, StdioError> {
    serde_json::from_str::<StdioTask>(json)
        .map_err(|e| serde_error_to_stdio_error("deserialize StdioTask", e))
}

pub fn stdio_tasks_to_json(tasks: &Vec<StdioTask>) -> Result<String, StdioError> {
    serde_json::to_string(tasks).map_err(|e| serde_error_to_stdio_error("serialize tasks", e))
}

pub fn stdio_tasks_from_json(json: &str) -> Result<Vec<StdioTask>, StdioError> {
    serde_json::from_str::<Vec<StdioTask>>(json)
        .map_err(|e| serde_error_to_stdio_error("deserialize tasks", e))
}

pub fn stdio_run_opts_to_json(opts: &StdioRunOpts) -> Result<String, StdioError> {
    serde_json::to_string(opts).map_err(|e| serde_error_to_stdio_error("serialize StdioRunOpts", e))
}

pub fn stdio_run_opts_to_pretty_json(opts: &StdioRunOpts) -> Result<String, StdioError> {
    serde_json::to_string_pretty(opts)
        .map_err(|e| serde_error_to_stdio_error("serialize StdioRunOpts", e))
}

pub fn stdio_run_opts_from_json(json: &str) -> Result<StdioRunOpts, StdioError> {
    serde_json::from_str::<StdioRunOpts>(json)
        .map_err(|e| serde_error_to_stdio_error("deserialize StdioRunOpts", e))
}

pub fn write_stdio_task_json_file(
    path: impl AsRef<Path>,
    task: &StdioTask,
) -> Result<(), StdioError> {
    let path = path.as_ref();
    let json = stdio_task_to_pretty_json(task)?;
    std::fs::write(path, json).map_err(|e| io_error_to_stdio_error(path, e))
}

pub fn read_stdio_task_json_file(path: impl AsRef<Path>) -> Result<StdioTask, StdioError> {
    let path = path.as_ref();
    let json = std::fs::read_to_string(path).map_err(|e| io_error_to_stdio_error(path, e))?;
    stdio_task_from_json(&json)
}

pub fn write_stdio_tasks_json_file(
    path: impl AsRef<Path>,
    tasks: &[StdioTask],
) -> Result<(), StdioError> {
    let path = path.as_ref();
    let json = serde_json::to_string_pretty(tasks)
        .map_err(|e| serde_error_to_stdio_error("serialize tasks", e))?;
    std::fs::write(path, json).map_err(|e| io_error_to_stdio_error(path, e))
}

pub fn read_stdio_tasks_json_file(path: impl AsRef<Path>) -> Result<Vec<StdioTask>, StdioError> {
    let path = path.as_ref();
    let json = std::fs::read_to_string(path).map_err(|e| io_error_to_stdio_error(path, e))?;
    stdio_tasks_from_json(&json)
}

pub fn write_stdio_run_opts_json_file(
    path: impl AsRef<Path>,
    opts: &StdioRunOpts,
) -> Result<(), StdioError> {
    let path = path.as_ref();
    let json = stdio_run_opts_to_pretty_json(opts)?;
    std::fs::write(path, json).map_err(|e| io_error_to_stdio_error(path, e))
}

pub fn read_stdio_run_opts_json_file(path: impl AsRef<Path>) -> Result<StdioRunOpts, StdioError> {
    let path = path.as_ref();
    let json = std::fs::read_to_string(path).map_err(|e| io_error_to_stdio_error(path, e))?;
    stdio_run_opts_from_json(&json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stdio_task_json_round_trip() {
        let task = StdioTask {
            id: "t1".to_string(),
            backend: "default".to_string(),
            workdir: "project".to_string(),
            model: Some("gpt-4o-mini".to_string()),
            model_provider: Some("openai".to_string()),
            dependencies: vec!["dep1".to_string()],
            stream_format: "text".to_string(),
            timeout: Some(123),
            retry: Some(2),
            files: vec!["README.md".to_string()],
            files_mode: super::super::FilesMode::Ref,
            files_encoding: super::super::FilesEncoding::Utf8,
            content: "hello".to_string(),
            backend_kind: Some(crate::config::BackendKind::Codecli),
            env_file: Some(".env".to_string()),
            env: Some(vec!["A=B".to_string()]),
            task_level: Some("normal".to_string()),
            resume_run_id: Some("run1".to_string()),
            resume_context: Some("ctx".to_string()),
        };

        let json = stdio_task_to_json(&task).unwrap();
        let decoded = stdio_task_from_json(&json).unwrap();
        assert_eq!(decoded.id, task.id);
        assert_eq!(decoded.files_mode, task.files_mode);
        assert_eq!(decoded.files_encoding, task.files_encoding);
        assert_eq!(decoded.backend_kind, task.backend_kind);
        assert_eq!(decoded.env, task.env);
    }

    #[test]
    fn stdio_run_opts_json_round_trip() {
        let opts = StdioRunOpts {
            stream_format: "text".to_string(),
            verbose: true,
            quiet: false,
            ascii: false,
            capture_bytes: 4096,
            resume_run_id: Some("run1".to_string()),
            resume_context: Some("ctx".to_string()),
        };

        let json = stdio_run_opts_to_json(&opts).unwrap();
        let decoded = stdio_run_opts_from_json(&json).unwrap();
        assert_eq!(decoded.stream_format, opts.stream_format);
        assert_eq!(decoded.capture_bytes, opts.capture_bytes);
        assert_eq!(decoded.resume_run_id, opts.resume_run_id);
    }

    #[test]
    fn stdio_task_json_file_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("task.json");

        let task = StdioTask {
            id: "t1".to_string(),
            backend: "default".to_string(),
            workdir: "project".to_string(),
            model: None,
            model_provider: None,
            dependencies: vec![],
            stream_format: "text".to_string(),
            timeout: None,
            retry: None,
            files: vec![],
            files_mode: super::super::FilesMode::Auto,
            files_encoding: super::super::FilesEncoding::Auto,
            content: "hello".to_string(),
            backend_kind: None,
            env_file: None,
            env: None,
            task_level: None,
            resume_run_id: None,
            resume_context: None,
        };

        write_stdio_task_json_file(&path, &task).unwrap();
        let decoded = read_stdio_task_json_file(&path).unwrap();
        assert_eq!(decoded.id, task.id);
        assert_eq!(decoded.content, task.content);
        assert_eq!(decoded.files_mode, task.files_mode);
    }
}
