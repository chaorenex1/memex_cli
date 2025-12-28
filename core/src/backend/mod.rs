use std::collections::HashMap;

use anyhow::Result;

use crate::runner::{RunnerPlugin, RunnerStartArgs};

pub struct BackendPlan {
    pub runner: Box<dyn RunnerPlugin>,
    pub session_args: RunnerStartArgs,
}

pub trait BackendStrategy: Send + Sync {
    fn name(&self) -> &str;

    #[allow(clippy::too_many_arguments)]
    fn plan(
        &self,
        backend: &str,
        base_envs: HashMap<String, String>,
        resume_id: Option<String>,
        prompt: String,
        model: Option<String>,
        stream: bool,
        stream_format: &str,
    ) -> Result<BackendPlan>;
}
