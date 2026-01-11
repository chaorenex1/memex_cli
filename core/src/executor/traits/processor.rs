use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use crate::config::AppConfig;
use crate::executor::types::{ExecutableTask, ProcessorError};

/// 任务处理器插件（在执行前转换任务）
#[async_trait]
pub trait TaskProcessorPlugin: Send + Sync {
    /// 插件名称（唯一标识）
    fn name(&self) -> &str;

    /// 处理优先级（数字越大越先执行）
    fn priority(&self) -> i32 {
        0
    }

    /// 处理任务
    async fn process(
        &self,
        task: &ExecutableTask,
        context: &ProcessContext,
    ) -> Result<ProcessedTask, ProcessorError>;

    /// 是否可并行执行（与其他处理器）
    fn is_parallelizable(&self) -> bool {
        true
    }
}

/// 处理上下文
#[derive(Debug, Clone)]
pub struct ProcessContext {
    pub dependency_outputs: HashMap<String, String>,
    pub dependency_results: HashMap<String, DependencyResult>,
    pub run_id: String,
    pub stage_id: usize,
    pub app_config: Arc<AppConfig>,
}

#[derive(Debug, Clone)]
pub struct DependencyResult {
    pub exit_code: i32,
    pub output: String,
}

/// 处理后的任务
#[derive(Debug, Clone)]
pub struct ProcessedTask {
    pub original: ExecutableTask,
    pub enhanced_content: String,
    pub metadata: ProcessMetadata,
}

#[derive(Debug, Clone, Default)]
pub struct ProcessMetadata {
    pub files: Vec<FileInfo>,
    pub custom: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub path: String,
    pub size: u64,
}
