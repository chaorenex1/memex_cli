//! 标准（非 TUI）执行流：解析用户输入、调用 plugins planner 生成 `RunnerSpec`，再通过 core 引擎执行一次会话。
use memex_core::api as core_api;
use std::sync::Arc;

use crate::commands::cli::{Args, RunArgs};
use crate::task_level::infer_task_level;
use memex_plugins::plan::{build_runner_spec, PlanMode, PlanRequest};

pub async fn run_standard_flow(
    args: &Args,
    run_args: Option<&RunArgs>,
    cfg: &mut core_api::AppConfig,
    events_out_tx: Option<core_api::EventsOutTx>,
    run_id: String,
    recover_run_id: Option<String>,
    stream_format: &str,
    policy: Option<Arc<dyn core_api::PolicyPlugin>>,
    memory: Option<Arc<dyn core_api::MemoryPlugin>>,
    gatekeeper: Arc<dyn core_api::GatekeeperPlugin>,
) -> Result<i32, core_api::RunnerError> {
    let user_query = resolve_user_query(args, run_args)?;
    let plan_req = build_plan_request(
        args,
        run_args,
        recover_run_id,
        stream_format,
    );
    let (runner_spec, start_data) = build_runner_spec(cfg, plan_req)?;

    core_api::run_with_query(
        core_api::RunWithQueryArgs {
            user_query,
            cfg: cfg.clone(),
            runner: runner_spec,
            run_id,
            capture_bytes: args.capture_bytes,
            stream_format: stream_format.to_string(),
            events_out_tx,
            policy,
            memory,
            gatekeeper,
            wrapper_start_data: start_data,
        },
        |input| async move {
            core_api::run_session(core_api::RunSessionArgs {
                session: input.session,
                control: &input.control,
                policy: input.policy,
                capture_bytes: input.capture_bytes,
                events_out: input.events_out_tx,
                event_tx: None,
                run_id: &input.run_id,
                backend_kind: &input.backend_kind,
                stream_format: &input.stream_format,
                abort_rx: None,
            })
            .await
        },
    )
    .await
}

fn resolve_user_query(
    args: &Args,
    run_args: Option<&RunArgs>,
) -> Result<String, core_api::RunnerError> {
    let mut prompt_text: Option<String> = None;

    if let Some(ra) = run_args {
        if let Some(prompt) = &ra.prompt {
            prompt_text = Some(prompt.clone());
        } else if let Some(path) = &ra.prompt_file {
            let content = std::fs::read_to_string(path).map_err(|e| {
                core_api::RunnerError::Spawn(format!("failed to read prompt file: {}", e))
            })?;
            prompt_text = Some(content);
        } else if ra.stdin {
            use std::io::Read;
            let mut content = String::new();
            std::io::stdin().read_to_string(&mut content).map_err(|e| {
                core_api::RunnerError::Spawn(format!("failed to read prompt from stdin: {}", e))
            })?;
            prompt_text = Some(content);
        }
    }

    Ok(prompt_text.unwrap_or_else(|| args.codecli_args.join(" ")))
}

fn build_plan_request(
    args: &Args,
    run_args: Option<&RunArgs>,
    recover_run_id: Option<String>,
    stream_format: &str,
) -> PlanRequest {
    let mode = match run_args {
        Some(ra) => {
            let backend_kind = ra.backend_kind.map(|kind| match kind {
                crate::commands::cli::BackendKind::Codecli => "codecli".to_string(),
                crate::commands::cli::BackendKind::Aiservice => "aiservice".to_string(),
            });

            let task_level = match ra.task_level {
                crate::commands::cli::TaskLevel::Auto => {
                    let prompt_for_level = ra
                        .prompt
                        .clone()
                        .unwrap_or_else(|| args.codecli_args.join(" "));
                    format!("{:?}", infer_task_level(&prompt_for_level))
                }
                lv => format!("{lv:?}"),
            };

            PlanMode::Backend {
                backend_spec: ra.backend.clone(),
                backend_kind,
                env_file: ra.env_file.clone(),
                env: ra.env.clone(),
                model: ra.model.clone(),
                task_level: Some(task_level),
            }
        }
        None => PlanMode::Legacy {
            cmd: args.codecli_bin.clone(),
            args: args.codecli_args.clone(),
        },
    };

    PlanRequest {
        mode,
        resume_id: recover_run_id,
        stream_format: stream_format.to_string(),
    }
}
