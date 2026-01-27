//! 标准（非 TUI）执行流：解析用户输入、调用 planner 生成 `RunnerSpec`，通过 core 引擎执行一次会话。
use crate::commands::cli::{Args, RunArgs};
use crate::http::client::RemoteClient;
use crate::stdio::{execute_stdio_tasks, read_stdin_text};
use memex_core::api as core_api;
use tokio::sync::mpsc;

pub async fn run_standard_flow(
    args: &Args,
    run_args: Option<&RunArgs>,
    ctx: &core_api::AppContext,
    is_remote: &bool,
    recover_run_id: Option<String>,
) -> Result<i32, core_api::RunnerError> {
    // Step 1: Read raw input from all sources
    let raw_input = read_raw_input(run_args)?;

    // Step 2: Parse input into tasks (structured or plain text mode)

    let run_id = recover_run_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let project_id =
        if let Some(project_id) = run_args.as_ref().and_then(|ra| ra.project_id.clone()) {
            project_id
        } else {
            let current_dir = std::env::current_dir().map_err(|e| {
                core_api::RunnerError::Config(format!(
                    "failed to determine project_id from current_dir fallback: {e}"
                ))
            })?;
            core_api::generate_project_id(&current_dir)
        };

    let stream_format = run_args
        .as_ref()
        .map(|ra| ra.stream_format.clone())
        .unwrap_or_else(|| "text".to_string());

    let backend_kind = run_args
        .as_ref()
        .and_then(|ra| ra.backend_kind)
        .map(Into::into);

    let env_file = run_args.as_ref().and_then(|ra| ra.env_file.clone());

    let env = run_args.as_ref().map(|ra| ra.env.clone());

    let mut tasks: Vec<core_api::StdioTask> = parse_input_to_tasks(&raw_input, run_args)?;
    // Step 3: Route based on task count
    // let user_query = tasks[0].content.clone();

    if tasks.is_empty() {
        tasks.push(core_api::StdioTask {
            id: run_id.clone(),
            content: raw_input.clone(),
            backend: run_args.map(|ra| ra.backend.clone()).unwrap_or_default(),
            model: run_args.and_then(|ra| ra.model.clone()),
            model_provider: run_args.and_then(|ra| ra.model_provider.clone()),
            workdir: project_id.clone(),
            stream_format: stream_format.clone(),
            dependencies: vec![],
            timeout: Some(300),
            retry: Some(1),
            files: vec![],
            files_encoding: core_api::FilesEncoding::Utf8,
            files_mode: core_api::FilesMode::Ref,
            backend_kind,
            env_file,
            env,
            task_level: None,
            resume_run_id: recover_run_id.clone(),
            resume_context: Some(raw_input.clone()),
        });
    } else {
        // For each task, fill in missing fields from run_args
        for task in tasks.iter_mut() {
            if task.id.is_empty() {
                task.id = run_id.clone();
            }
            if task.workdir.is_empty() {
                task.workdir = project_id.clone();
            }
            if task.stream_format.is_empty() {
                task.stream_format = stream_format.clone();
            }
            if task.backend_kind.is_none() {
                task.backend_kind = backend_kind;
            }
            if task.env_file.is_none() {
                task.env_file = env_file.clone();
            }
            if task.env.is_none() {
                task.env = env.clone();
            }
            if task.resume_run_id.is_none() {
                task.resume_run_id = recover_run_id.clone();
            }
            if task.resume_context.is_none() {
                task.resume_context = Some(raw_input.clone());
            }
        }
    }
    // Multiple tasks: use run_stdio
    tracing::info!(
        "Executing {} tasks... on project_id={} mode={}",
        tasks.len(),
        &project_id,
        if *is_remote { "remote" } else { "local" }
    );
    let stdio_opts: core_api::StdioRunOpts = core_api::StdioRunOpts {
        stream_format: stream_format.clone(),
        capture_bytes: args.capture_bytes,
        quiet: false,
        verbose: true,
        ascii: false,
        resume_run_id: recover_run_id.clone(),
        resume_context: Some(raw_input.clone()),
    };
    if *is_remote {
        let server_url = format!(
            "http://{}:{}",
            ctx.cfg().http_server.host,
            ctx.cfg().http_server.port
        );
        let client = RemoteClient::from_config(&server_url);
        client.exec_run(&tasks, &stdio_opts).await
    } else {
        // 本地模式：直接调用 Core
        run_multi_tasks(&tasks, &stdio_opts, ctx, None).await
    }
}

/// Reads raw input from all possible sources (--prompt, --prompt-file, --stdin, args)
fn read_raw_input(run_args: Option<&RunArgs>) -> Result<String, core_api::RunnerError> {
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
            let content = read_stdin_text().map_err(|e| {
                core_api::RunnerError::Spawn(format!("failed to read prompt from stdin: {}", e))
            })?;
            prompt_text = Some(content);
        }
    }

    Ok(prompt_text.unwrap_or("".to_string()))
}

/// Parses raw input into a list of StdioTask using InputParser
fn parse_input_to_tasks(
    raw_input: &str,
    run_args: Option<&RunArgs>,
) -> Result<Vec<core_api::StdioTask>, core_api::RunnerError> {
    // Determine structured mode (default: true)
    let structured = run_args.map(|ra| ra.structured_text).unwrap_or(true);

    // Parse using InputParser
    core_api::InputParser::parse(raw_input, structured).map_err(|e| {
        core_api::RunnerError::Spawn(format!("failed to parse input into tasks: {}", e))
    })
}

/// Executes multiple tasks using new executor with dependency graph support
pub async fn run_multi_tasks(
    tasks: &Vec<core_api::StdioTask>,
    stdio_opts: &core_api::StdioRunOpts,
    ctx: &core_api::AppContext,
    http_sse_tx: Option<mpsc::UnboundedSender<Vec<u8>>>,
) -> Result<i32, core_api::RunnerError> {
    let result = execute_stdio_tasks(tasks, ctx, stdio_opts, http_sse_tx)
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
