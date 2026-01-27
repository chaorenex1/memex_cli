use std::collections::HashMap;
use std::sync::Arc;

use crate::backend::BackendStrategy;
use crate::config::{AppConfig, BackendKind};
use crate::context::Services;
use crate::events_out::EventsOutTx;
use crate::runner::{PolicyPlugin, RunnerPlugin, RunnerSession, RunnerStartArgs};

pub struct RunSessionInput {
    pub session: Box<dyn RunnerSession>,
    pub run_id: String,
    pub control: crate::config::ControlConfig,
    pub policy: Option<Arc<dyn PolicyPlugin>>,
    pub capture_bytes: usize,
    pub events_out_tx: Option<EventsOutTx>,
    pub backend_kind: BackendKind,
    pub stream_format: String,
    pub stdin_payload: Option<String>,
}

pub enum RunnerSpec {
    Backend {
        strategy: Box<dyn BackendStrategy>,
        backend_spec: String,
        base_envs: HashMap<String, String>,
        resume_id: Option<String>,
        model: Option<String>,
        model_provider: Option<String>,
        project_id: Option<String>,
        stream_format: String,
        task_level: Option<String>,
    },
    Passthrough {
        runner: Box<dyn RunnerPlugin>,
        session_args: RunnerStartArgs,
    },
}

pub struct RunWithQueryArgs {
    pub user_query: String,
    pub cfg: AppConfig,
    pub runner: RunnerSpec,
    pub run_id: String,
    pub capture_bytes: usize,
    pub stream_format: String,
    pub project_id: String,
    pub events_out_tx: Option<EventsOutTx>,
    pub services: Services,
    pub wrapper_start_data: Option<serde_json::Value>,
}
