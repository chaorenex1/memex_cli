use memex_core::api as core_api;

pub async fn infer_task_level(
    prompt: &str,
    model: &str,
    model_provider: &str,
    query_services: &core_api::Services,
) -> core_api::TaskGradeResult {
    let s = prompt.trim();
    let Some(memory_plugin) = query_services.memory.as_ref() else {
        return core_api::TaskGradeResult {
            task_level: "L1".to_string(),
            reason: "Memory plugin not available".to_string(),
            recommended_model: model.to_string(),
            recommended_model_provider: Some(model_provider.to_string()),
            confidence: 0.0,
        };
    };
    let result = match memory_plugin.task_grade(s.to_string()).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("Failed to infer task level: {}", e);
            core_api::TaskGradeResult {
                task_level: "L1".to_string(),
                reason: "Failed to infer task level".to_string(),
                recommended_model: model.to_string(),
                recommended_model_provider: Some(model_provider.to_string()),
                confidence: 0.0,
            }
        }
    };
    result
}
