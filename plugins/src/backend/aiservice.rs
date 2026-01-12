use anyhow::{anyhow, Result};

use memex_core::api as core_api;

use crate::runner::aiservice::AiServiceRunnerPlugin;

pub struct AiServiceBackendStrategy;

impl core_api::BackendStrategy for AiServiceBackendStrategy {
    fn name(&self) -> &str {
        "aiservice"
    }

    fn plan(&self, request: core_api::BackendPlanRequest) -> Result<core_api::BackendPlan> {
        let core_api::BackendPlanRequest {
            backend,
            mut base_envs,
            resume_id: _resume_id,
            prompt,
            model,
            model_provider,
            project_id,
            stream_format,
        } = request;

        tracing::debug!("AiServiceBackendStrategy planning with backend: {}, project_id: {:?}, model: {:?}, model_provider: {:?}", backend, project_id, model, model_provider);
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
        // Wrapper always streams output; the format is controlled separately via stream_format.
        base_envs.insert("MEMEX_STREAM".to_string(), "1".to_string());
        base_envs.insert("MEMEX_STREAM_FORMAT".to_string(), stream_format);

        Ok(core_api::BackendPlan {
            runner: Box::new(AiServiceRunnerPlugin::new()),
            session_args: core_api::RunnerStartArgs {
                // cmd holds the endpoint URL for AiServiceRunnerPlugin
                cmd: backend.to_string(),
                // args[0] holds the prompt
                args: vec![prompt],
                envs: base_envs,
                cwd: None,
                stdin_payload: None,
            },
        })
    }
}
