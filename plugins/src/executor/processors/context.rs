use async_trait::async_trait;
use memex_core::executor::traits::{
    ProcessContext, ProcessMetadata, ProcessedTask, TaskProcessorPlugin,
};
use memex_core::executor::types::{ExecutableTask, ProcessorError};

/// Injects dependency outputs into task content.
pub struct ContextInjectorPlugin;

impl ContextInjectorPlugin {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ContextInjectorPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TaskProcessorPlugin for ContextInjectorPlugin {
    fn name(&self) -> &str {
        "context-injector"
    }

    fn priority(&self) -> i32 {
        20
    }

    async fn process(
        &self,
        task: &ExecutableTask,
        context: &ProcessContext,
    ) -> Result<ProcessedTask, ProcessorError> {
        if context.dependency_outputs.is_empty() && context.dependency_results.is_empty() {
            return Ok(ProcessedTask {
                original: task.clone(),
                enhanced_content: task.content.clone(),
                metadata: ProcessMetadata::default(),
            });
        }

        let mut injected = String::from("=== Dependency Outputs ===\n\n");

        let mut added = false;

        if !context.dependency_results.is_empty() {
            let mut dep_ids: Vec<&String> = context.dependency_results.keys().collect();
            dep_ids.sort();

            for dep_id in dep_ids {
                if let Some(result) = context.dependency_results.get(dep_id) {
                    injected.push_str(&format!("# Task: {}\n", dep_id));
                    injected.push_str(&format!("Exit Code: {}\n", result.exit_code));
                    if !result.output.is_empty() {
                        injected.push_str("Output:\n");
                        injected.push_str(&result.output);
                        if !result.output.ends_with('\n') {
                            injected.push('\n');
                        }
                        injected.push('\n');
                    }
                    added = true;
                }
            }
        } else {
            let mut dep_ids: Vec<&String> = context.dependency_outputs.keys().collect();
            dep_ids.sort();

            for dep_id in dep_ids {
                if let Some(output) = context.dependency_outputs.get(dep_id) {
                    if output.is_empty() {
                        continue;
                    }
                    injected.push_str(&format!("# Task: {}\n", dep_id));
                    injected.push_str("Output:\n");
                    injected.push_str(output);
                    if !output.ends_with('\n') {
                        injected.push('\n');
                    }
                    injected.push('\n');
                    added = true;
                }
            }
        }

        if !added {
            return Ok(ProcessedTask {
                original: task.clone(),
                enhanced_content: task.content.clone(),
                metadata: ProcessMetadata::default(),
            });
        }

        injected.push_str("=== End Dependency Outputs ===\n");

        let enhanced = format!("{}\n\n{}", injected, task.content);

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
    use memex_core::executor::traits::DependencyResult;
    use memex_core::api::AppConfig;
    use std::collections::HashMap;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_context_injector_inserts_outputs() {
        let plugin = ContextInjectorPlugin::new();
        let task = ExecutableTask::new("t1".to_string(), "body".to_string());

        let mut outputs = HashMap::new();
        outputs.insert("a".to_string(), "out-a".to_string());
        outputs.insert("b".to_string(), "".to_string());

        let mut results = HashMap::new();
        results.insert(
            "a".to_string(),
            DependencyResult {
                exit_code: 0,
                output: "out-a".to_string(),
            },
        );

        let ctx = ProcessContext {
            dependency_outputs: outputs,
            dependency_results: results,
            run_id: "run".to_string(),
            stage_id: 0,
            app_config: Arc::new(AppConfig::default()),
        };

        let result = plugin.process(&task, &ctx).await.unwrap();
        assert!(result.enhanced_content.contains("=== Dependency Outputs ==="));
        assert!(result.enhanced_content.contains("# Task: a"));
        assert!(result.enhanced_content.contains("out-a"));
        assert!(!result.enhanced_content.contains("# Task: b"));
        assert!(result.enhanced_content.ends_with("body"));
    }
}
