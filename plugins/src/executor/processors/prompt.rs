use async_trait::async_trait;
use memex_core::executor::traits::{
    ProcessContext, ProcessMetadata, ProcessedTask, TaskProcessorPlugin,
};
use memex_core::executor::types::{ExecutableTask, ProcessorError};

/// Simple prompt enhancer (no-op by default).
pub struct PromptEnhancerPlugin {
    prefix: Option<String>,
    suffix: Option<String>,
}

impl PromptEnhancerPlugin {
    pub fn new() -> Self {
        Self {
            prefix: None,
            suffix: None,
        }
    }

    pub fn with_prefix_suffix(prefix: Option<String>, suffix: Option<String>) -> Self {
        Self { prefix, suffix }
    }
}

impl Default for PromptEnhancerPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TaskProcessorPlugin for PromptEnhancerPlugin {
    fn name(&self) -> &str {
        "prompt-enhancer"
    }

    fn priority(&self) -> i32 {
        10
    }

    async fn process(
        &self,
        task: &ExecutableTask,
        _context: &ProcessContext,
    ) -> Result<ProcessedTask, ProcessorError> {
        let mut enhanced = String::new();

        if let Some(prefix) = &self.prefix {
            enhanced.push_str(prefix);
            if !enhanced.ends_with('\n') {
                enhanced.push('\n');
            }
        }

        enhanced.push_str(&task.content);

        if let Some(suffix) = &self.suffix {
            if !enhanced.ends_with('\n') {
                enhanced.push('\n');
            }
            enhanced.push_str(suffix);
        }

        Ok(ProcessedTask {
            original: task.clone(),
            enhanced_content: enhanced,
            metadata: ProcessMetadata::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memex_core::api::AppConfig;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn context() -> ProcessContext {
        ProcessContext {
            dependency_outputs: HashMap::new(),
            dependency_results: HashMap::new(),
            run_id: "run".to_string(),
            stage_id: 0,
            app_config: Arc::new(AppConfig::default()),
        }
    }

    #[tokio::test]
    async fn test_prompt_enhancer_noop() {
        let plugin = PromptEnhancerPlugin::new();
        let task = ExecutableTask::new("t1".to_string(), "hello".to_string());
        let result = plugin.process(&task, &context()).await.unwrap();
        assert_eq!(result.enhanced_content, "hello");
    }

    #[tokio::test]
    async fn test_prompt_enhancer_prefix_suffix() {
        let plugin = PromptEnhancerPlugin::with_prefix_suffix(
            Some("prefix".to_string()),
            Some("suffix".to_string()),
        );
        let task = ExecutableTask::new("t1".to_string(), "body".to_string());
        let result = plugin.process(&task, &context()).await.unwrap();
        assert_eq!(result.enhanced_content, "prefix\nbody\nsuffix");
    }
}
