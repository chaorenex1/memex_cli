/// Minimal, executor-agnostic task representation.
#[derive(Debug, Clone)]
pub struct ExecutableTask {
    pub id: String,
    pub content: String,
    pub dependencies: Vec<String>,
    pub metadata: TaskMetadata,
}

impl ExecutableTask {
    pub fn new(id: String, content: String) -> Self {
        Self {
            id,
            content,
            dependencies: Vec::new(),
            metadata: TaskMetadata::default(),
        }
    }
}

/// Common task interface for executor graph handling.
pub trait TaskLike: Clone + Send + Sync {
    fn id(&self) -> &str;
    fn dependencies(&self) -> &[String];
}

impl TaskLike for ExecutableTask {
    fn id(&self) -> &str {
        &self.id
    }

    fn dependencies(&self) -> &[String] {
        &self.dependencies
    }
}

/// Optional metadata attached to tasks, kept generic for plugin use.
#[derive(Debug, Clone, Default)]
pub struct TaskMetadata {
    pub backend: Option<String>,
    pub workdir: Option<String>,
    pub model: Option<String>,
    pub model_provider: Option<String>,
    pub stream_format: Option<String>,
    pub timeout: Option<u64>,
    pub retry: Option<u32>,
    pub files: Vec<String>,
    pub files_mode: Option<String>,
    pub files_encoding: Option<String>,
    pub tags: Vec<String>,
}
