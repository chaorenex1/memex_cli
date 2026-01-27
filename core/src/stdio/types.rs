use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilesMode {
    Embed,
    Ref,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilesEncoding {
    Utf8,
    Base64,
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StdioTask {
    pub id: String,
    pub backend: String,
    pub workdir: String,
    pub model: Option<String>,
    pub model_provider: Option<String>,
    pub dependencies: Vec<String>,
    pub stream_format: String,
    pub timeout: Option<u64>,
    pub retry: Option<u32>,
    pub files: Vec<String>,
    pub files_mode: FilesMode,
    pub files_encoding: FilesEncoding,
    pub content: String,
    pub backend_kind: Option<crate::config::BackendKind>,
    pub env_file: Option<String>,
    pub env: Option<Vec<String>>,
    pub task_level: Option<String>,
    pub resume_run_id: Option<String>,
    pub resume_context: Option<String>,
}

impl StdioTask {
    /// Convert legacy STDIO task into executor-agnostic representation.
    pub fn to_executable_task(&self) -> crate::executor::types::ExecutableTask {
        let mut task =
            crate::executor::types::ExecutableTask::new(self.id.clone(), self.content.clone());

        task.dependencies = self.dependencies.clone();
        task.metadata = crate::executor::types::TaskMetadata {
            backend: Some(self.backend.clone()),
            workdir: Some(self.workdir.clone()),
            model: self.model.clone(),
            model_provider: self.model_provider.clone(),
            stream_format: Some(self.stream_format.clone()),
            timeout: self.timeout,
            retry: self.retry,
            files: self.files.clone(),
            files_mode: Some(
                match self.files_mode {
                    FilesMode::Embed => "embed",
                    FilesMode::Ref => "ref",
                    FilesMode::Auto => "auto",
                }
                .to_string(),
            ),
            files_encoding: Some(
                match self.files_encoding {
                    FilesEncoding::Utf8 => "utf8",
                    FilesEncoding::Base64 => "base64",
                    FilesEncoding::Auto => "auto",
                }
                .to_string(),
            ),
            tags: Vec::new(),
        };

        task
    }
}

impl crate::executor::types::TaskLike for StdioTask {
    fn id(&self) -> &str {
        &self.id
    }

    fn dependencies(&self) -> &[String] {
        &self.dependencies
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StdioRunOpts {
    pub stream_format: String,
    pub verbose: bool,
    pub quiet: bool,
    pub ascii: bool,
    pub capture_bytes: usize,
    pub resume_run_id: Option<String>,
    pub resume_context: Option<String>,
}
