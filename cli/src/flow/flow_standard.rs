use memex_core::config::AppConfig;
use memex_core::engine;
use memex_core::error::RunnerError;
use memex_core::events_out::EventsOutTx;
use memex_core::memory::MemoryPlugin;
use memex_core::runner::{run_session, PolicyPlugin, RunSessionArgs};
use memex_core::state::StateManager;
use std::sync::Arc;

use crate::commands::cli::{Args, RunArgs};
use crate::flow::flow_qa::build_runner_spec;

pub async fn run_standard_flow(
    args: &Args,
    run_args: Option<&RunArgs>,
    cfg: &mut AppConfig,
    state_manager: Option<Arc<StateManager>>,
    events_out_tx: Option<EventsOutTx>,
    run_id: String,
    recover_run_id: Option<String>,
    stream_enabled: bool,
    stream_format: &str,
    stream_silent: bool,
    policy: Option<Box<dyn PolicyPlugin>>,
    memory: Option<Box<dyn MemoryPlugin>>,
    gatekeeper: Box<dyn memex_core::gatekeeper::GatekeeperPlugin>,
) -> Result<i32, RunnerError> {
    let user_query = resolve_user_query(args, run_args)?;
    let (runner_spec, start_data) = build_runner_spec(
        args,
        run_args,
        cfg,
        recover_run_id.clone(),
        stream_enabled,
        stream_format,
    )?;

    engine::run_with_query(
        engine::RunWithQueryArgs {
            user_query,
            cfg: cfg.clone(),
            runner: runner_spec,
            run_id,
            capture_bytes: args.capture_bytes,
            silent: stream_silent,
            events_out_tx,
            state_manager,
            policy,
            memory,
            gatekeeper,
            wrapper_start_data: start_data,
        },
        |input| async move {
            run_session(RunSessionArgs {
                session: input.session,
                control: &input.control,
                policy: input.policy,
                capture_bytes: input.capture_bytes,
                events_out: input.events_out_tx,
                event_tx: None,
                run_id: &input.run_id,
                silent: input.silent,
                state_manager: input.state_manager,
                session_id: input.state_session_id,
            })
            .await
        },
    )
    .await
}

fn resolve_user_query(args: &Args, run_args: Option<&RunArgs>) -> Result<String, RunnerError> {
    let mut prompt_text: Option<String> = None;

    if let Some(ra) = run_args {
        if let Some(prompt) = &ra.prompt {
            prompt_text = Some(prompt.clone());
        } else if let Some(path) = &ra.prompt_file {
            let content = std::fs::read_to_string(path)
                .map_err(|e| RunnerError::Spawn(format!("failed to read prompt file: {}", e)))?;
            prompt_text = Some(content);
        } else if ra.stdin {
            use std::io::Read;
            let mut content = String::new();
            std::io::stdin().read_to_string(&mut content).map_err(|e| {
                RunnerError::Spawn(format!("failed to read prompt from stdin: {}", e))
            })?;
            prompt_text = Some(content);
        }
    }

    Ok(prompt_text.unwrap_or_else(|| args.codecli_args.join(" ")))
}
