use std::collections::HashMap;

use anyhow::Result;

use crate::runner::{RunnerPlugin, RunnerStartArgs};

pub struct BackendPlan {
    pub runner: Box<dyn RunnerPlugin>,
    pub session_args: RunnerStartArgs,
}

/// Request parameters for backend planning
#[derive(Debug, Clone)]
pub struct BackendPlanRequest {
    pub backend: String,
    pub base_envs: HashMap<String, String>,
    pub resume_id: Option<String>,
    pub prompt: String,
    pub model: Option<String>,
    pub model_provider: Option<String>,
    pub project_id: Option<String>,
    pub stream_format: String,
    pub task_level: Option<String>,
}

pub trait BackendStrategy: Send + Sync {
    fn name(&self) -> &str;

    fn plan(&self, request: BackendPlanRequest) -> Result<BackendPlan>;
}
