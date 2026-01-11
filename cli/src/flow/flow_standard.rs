//! 标准（非 TUI）执行流：解析用户输入、调用 planner 生成 `RunnerSpec`，通过 core 引擎执行一次会话。
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
    // Step 1: Read raw input from all sources
    let raw_input = read_raw_input(args, run_args)?;

    // Step 2: Parse input into tasks (structured or plain text mode)
    let tasks = parse_input_to_tasks(&raw_input, run_args)?;

    // Step 3: Route based on task count
    if tasks.len() == 1 {
        // Single task: extract content and use original flow
        let user_query = tasks[0].content.clone();

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
                let backend_kind_str = input.backend_kind.to_string();
                core_api::run_session(core_api::RunSessionArgs {
                    session: input.session,
                    control: &input.control,
                    policy: input.policy,
                    capture_bytes: input.capture_bytes,
                    events_out: input.events_out_tx,
                    event_tx: None,
                    run_id: &input.run_id,
                    backend_kind: &backend_kind_str,
                    stream_format: &input.stream_format,
                    abort_rx: None,
                })
                .await
            },
        )
        .await
    } else {
        // Multiple tasks: use run_stdio
        tracing::info!(
            "Parsed {} tasks from structured input, using STDIO executor",
            tasks.len()
        );
        run_multi_tasks(tasks, args, cfg, stream_format, services).await
    }
}

/// Reads raw input from all possible sources (--prompt, --prompt-file, --stdin, args)
fn read_raw_input(
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

/// Parses raw input into a list of StdioTask using InputParser
fn parse_input_to_tasks(
    raw_input: &str,
    run_args: Option<&RunArgs>,
) -> Result<Vec<core_api::StdioTask>, core_api::RunnerError> {
    // Determine structured mode (default: true)
    let structured = run_args.map(|ra| ra.structured_text).unwrap_or(true);

    // Extract defaults for plain text mode
    let default_backend = run_args
        .map(|ra| ra.backend.clone())
        .unwrap_or_else(|| "codex".to_string());

    let default_workdir = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string());

    let default_model = run_args.and_then(|ra| ra.model.clone());

    let default_stream_format = run_args
        .map(|ra| ra.stream_format.clone())
        .unwrap_or_else(|| "text".to_string());

    // Parse using InputParser
    core_api::InputParser::parse(
        raw_input,
        structured,
        &default_backend,
        &default_workdir,
        default_model,
        &default_stream_format,
    )
    .map_err(|e| core_api::RunnerError::Spawn(format!("failed to parse input into tasks: {}", e)))
}

/// Executes multiple tasks using new executor with dependency graph support
async fn run_multi_tasks(
    tasks: Vec<core_api::StdioTask>,
    args: &Args,
    cfg: &mut core_api::AppConfig,
    stream_format: &str,
    _services: &core_api::Services,
) -> Result<i32, core_api::RunnerError> {
    // Build AppContext
    let ctx = core_api::AppContext::new(cfg.clone(), None)
        .await
        .map_err(|e| core_api::RunnerError::Config(e.to_string()))?;

    // Convert to ExecutionOpts (new executor API)
    let opts = core_api::ExecutionOpts {
        stream_format: stream_format.to_string(),
        capture_bytes: args.capture_bytes,
        verbose: false,     // Use default
        quiet: false,       // Use default
        ascii: false,       // Use default
        max_parallel: None, // Use default from config
        resume_run_id: None,
        resume_context: None,
        progress_bar: stream_format == "text", // Enable progress bar for text output
        // STDIO optimization defaults (not needed for non-STDIO flow, but required for struct)
        enable_event_buffering: false,
        event_buffer_size: 100,
        event_flush_interval_ms: 100,
        enable_adaptive_concurrency: false,
        enable_file_cache: false,
        enable_mmap_large_files: false,
        mmap_threshold_mb: 10,
    };

    // Planner: builds RunnerSpec for each task
    let cfg_for_planner = cfg.clone();
    let planner = move |task: &core_api::StdioTask| -> Result<
        (core_api::RunnerSpec, Option<serde_json::Value>),
        core_api::StdioError,
    > {
        let mut task_cfg = cfg_for_planner.clone();

        let plan_req = PlanRequest {
            mode: PlanMode::Backend {
                backend_spec: task.backend.clone(),
                backend_kind: None, // Auto-detect from backend string
                env_file: None,
                env: vec![],
                model: task.model.clone(),
                model_provider: task.model_provider.clone(),
                project_id: Some(task.workdir.clone()),
                task_level: None,
            },
            resume_id: None,
            stream_format: task.stream_format.clone(),
        };

        let (runner_spec, start_data) = build_runner_spec(&mut task_cfg, plan_req)
            .map_err(|e| core_api::StdioError::BackendError(e.to_string()))?;

        Ok((runner_spec, start_data))
    };

    // Call new executor API with planner parameter
    let result = core_api::execute_tasks(tasks, &ctx, &opts, planner)
        .await
        .map_err(|e| core_api::RunnerError::Stdio(e.to_string()))?;

    // Convert ExecutionResult to exit code
    if result.failed > 0 {
        tracing::error!(
            "❌ Execution failed: {}/{} tasks failed",
            result.failed,
            result.total_tasks
        );
        Ok(1)
    } else {
        tracing::info!(
            "✅ Execution successful: {} tasks completed in {}ms",
            result.completed,
            result.duration_ms
        );
        Ok(0)
    }
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
            let backend_kind = ra.backend_kind.map(Into::into);

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
