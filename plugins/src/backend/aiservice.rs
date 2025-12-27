use std::collections::HashMap;

use anyhow::{anyhow, Result};

use memex_core::backend::{BackendPlan, BackendStrategy};
use memex_core::runner::RunnerStartArgs;

use crate::runner::aiservice::AiServiceRunnerPlugin;

pub struct AiServiceBackendStrategy;

impl BackendStrategy for AiServiceBackendStrategy {
    fn name(&self) -> &str {
        "aiservice"
    }

    fn plan(
        &self,
        backend: &str,
        mut base_envs: HashMap<String, String>,
        _resume_id: Option<String>,
        prompt: String,
        model: Option<String>,
        stream: bool,
        stream_format: &str,
    ) -> Result<BackendPlan> {
        if !(backend.starts_with("http://") || backend.starts_with("https://")) {
            return Err(anyhow!(
                "aiservice backend must be a URL (http/https), got: {}",
                backend
            ));
        }

        // Use env vars to pass metadata without overloading RunnerStartArgs.
        if let Some(m) = &model {
            base_envs.insert("MEMEX_MODEL".to_string(), m.clone());
        }
        base_envs.insert(
            "MEMEX_STREAM".to_string(),
            if stream { "1" } else { "0" }.to_string(),
        );
        base_envs.insert("MEMEX_STREAM_FORMAT".to_string(), stream_format.to_string());

        Ok(BackendPlan {
            runner: Box::new(AiServiceRunnerPlugin::new()),
            session_args: RunnerStartArgs {
                // cmd holds the endpoint URL for AiServiceRunnerPlugin
                cmd: backend.to_string(),
                // args[0] holds the prompt
                args: vec![prompt],
                envs: base_envs,
            },
        })
    }
}
