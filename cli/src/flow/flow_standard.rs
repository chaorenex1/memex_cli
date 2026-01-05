//! 标准（非 TUI）执行流：解析用户输入、调用 plugins planner 生成 `RunnerSpec`，再通过 core 引擎执行一次会话。
use crate::commands::cli::{Args, RunArgs};
use crate::task_level::infer_task_level;
use memex_core::api as core_api;
use memex_plugins::plan::{build_runner_spec, PlanMode, PlanRequest};

pub async fn run_standard_flow(
    args: &Args,
    run_args: Option<&RunArgs>,
    cfg: &mut core_api::AppConfig,
    events_out_tx: Option<core_api::EventsOutTx>,
    run_id: String,
    recover_run_id: Option<String>,
    stream_format: &str,
    project_id: &str,
    services: &core_api::Services,
) -> Result<i32, core_api::RunnerError> {
    let user_query = resolve_user_query(args, run_args)?;
    let plan_req = build_plan_request(
        services,
        args,
        run_args,
        recover_run_id,
        stream_format,
        project_id,
        &user_query,
    )
    .await;
    let (runner_spec, start_data) = build_runner_spec(cfg, plan_req)?;

    core_api::run_with_query(
        core_api::RunWithQueryArgs {
            user_query,
            cfg: cfg.clone(),
            runner: runner_spec,
            run_id,
            capture_bytes: args.capture_bytes,
            stream_format: stream_format.to_string(),
            project_id: project_id.to_string(),
            events_out_tx,
            services: services.clone(),
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

async fn build_plan_request(
    query_services: &core_api::Services,
    args: &Args,
    run_args: Option<&RunArgs>,
    recover_run_id: Option<String>,
    stream_format: &str,
    project_id: &str,
    user_query: &str,
) -> PlanRequest {
    let mode = match run_args {
        Some(ra) => {
            let backend_kind = ra.backend_kind.map(|kind| match kind {
                crate::commands::cli::BackendKind::Codecli => "codecli".to_string(),
                crate::commands::cli::BackendKind::Aiservice => "aiservice".to_string(),
            });

            if ra.backend == "codex" && ra.model_provider.is_some() {
                let task_grade_result = infer_task_level(
                    user_query,
                    ra.model.as_deref().unwrap_or(""),
                    ra.model_provider.as_deref().unwrap_or(""),
                    query_services,
                )
                .await;
                tracing::info!(
                    " Inferred task level: {}, reason: {}, recommended model: {}, confidence: {}",
                    task_grade_result.task_level,
                    task_grade_result.reason,
                    task_grade_result.recommended_model,
                    task_grade_result.confidence,
                );
                PlanMode::Backend {
                    backend_spec: ra.backend.clone(),
                    backend_kind,
                    env_file: ra.env_file.clone(),
                    env: ra.env.clone(),
                    model: task_grade_result.recommended_model.clone().into(),
                    model_provider: task_grade_result.recommended_model_provider.clone(),
                    project_id: Some(project_id.to_string()),
                    task_level: Some(task_grade_result.task_level.to_string()),
                }
            } else {
                PlanMode::Backend {
                    backend_spec: ra.backend.clone(),
                    backend_kind,
                    env_file: ra.env_file.clone(),
                    env: ra.env.clone(),
                    model: ra.model.clone().unwrap_or_default().into(),
                    model_provider: ra.model_provider.clone(),
                    project_id: Some(project_id.to_string()),
                    task_level: None,
                }
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
