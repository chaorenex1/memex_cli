use std::fs;
use std::io::Read;

use memex_core::api as core_api;
use memex_plugins::factory;
use memex_plugins::plan::{build_runner_spec, PlanMode, PlanRequest};
use uuid::Uuid;

use crate::commands::cli::StdioArgs;

pub async fn handle_stdio(
    args: StdioArgs,
    capture_bytes: usize,
    ctx: &core_api::AppContext,
) -> Result<i32, core_api::CliError> {
    if args.quiet && args.verbose {
        return Err(core_api::CliError::Command(
            "--quiet and --verbose are mutually exclusive".to_string(),
        ));
    }

    if args.run_id.is_some() && args.events_file.is_none() {
        return Err(core_api::CliError::Command(
            "--run-id requires --events-file".to_string(),
        ));
    }

    if args.events_file.is_some() && args.run_id.is_none() {
        return Err(core_api::CliError::Command(
            "--events-file requires --run-id".to_string(),
        ));
    }

    let input = match args.input_file.as_deref() {
        Some(path) => fs::read_to_string(path).map_err(core_api::CliError::Io)?,
        None => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .map_err(core_api::CliError::Io)?;
            buf
        }
    };

    let mut tasks = match core_api::parse_stdio_tasks(&input) {
        Ok(t) => t,
        Err(e) => {
            let code = e.error_code().as_u16() as i32;
            emit_stdio_error(&args, &e, None);
            return Ok(code);
        }
    };

    // Load resume context if provided
    let resume_context =
        if let (Some(run_id), Some(events_file)) = (&args.run_id, &args.events_file) {
            match load_resume_context(events_file, run_id) {
                Ok(ctx) => Some(ctx),
                Err(e) => {
                    eprintln!("Failed to load resume context: {}", e);
                    return Ok(1);
                }
            }
        } else {
            None
        };

    // CLI stream-format overrides per-task defaults to keep output consistent.
    for t in tasks.iter_mut() {
        t.stream_format = args.stream_format.clone();
    }

    // ===== 新执行器调用开始 =====

    // 配置STDIO事件缓冲（减少JSONL输出系统调用）
    core_api::configure_event_buffer(
        ctx.cfg().stdio.enable_event_buffering,
        ctx.cfg().stdio.event_buffer_size,
        ctx.cfg().stdio.event_flush_interval_ms,
    );

    // 构建StdioRunOpts
    let stdio_opts = core_api::StdioRunOpts {
        stream_format: args.stream_format.clone(),
        ascii: args.ascii,
        verbose: args.verbose,
        quiet: args.quiet,
        capture_bytes,
        resume_run_id: args.run_id.clone(),
        resume_context: resume_context.clone(),
    };

    // 构建ExecutionOpts（从StdioConfig扩展STDIO优化配置）
    let exec_opts = core_api::ExecutionOpts::from_stdio_config(&stdio_opts, &ctx.cfg().stdio);

    // 定义planner（构建RunnerSpec）
    // 使用Arc包装ctx以使闭包可Clone
    let ctx_for_planner = std::sync::Arc::new(ctx.clone());
    let planner = move |task: &core_api::StdioTask| -> Result<
        (core_api::RunnerSpec, Option<serde_json::Value>),
        core_api::StdioError,
    > {
        let mut cfg = ctx_for_planner.cfg().clone();
        let plan_req = PlanRequest {
            mode: PlanMode::Backend {
                backend_spec: task.backend.clone(),
                backend_kind: None,
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
        let (runner_spec, start_data) = build_runner_spec(&mut cfg, plan_req)
            .map_err(|e| core_api::StdioError::BackendError(e.to_string()))?;
        Ok((runner_spec, start_data))
    };

    // 构建插件（通过 factory）
    let processors = factory::build_task_processors(&ctx.cfg().executor);
    let renderer = factory::build_renderer(&args.stream_format, &ctx.cfg().executor.output);
    let retry_strategy = factory::build_retry_strategy(&ctx.cfg().executor.retry);
    let concurrency_strategy =
        factory::build_concurrency_strategy(&ctx.cfg().executor.concurrency);

    // 注入resume上下文到第一个任务
    if let Some(ctx_str) = &resume_context {
        if !ctx_str.is_empty() && !tasks.is_empty() {
            tasks[0].content = format!("{}{}", ctx_str, tasks[0].content);
        }
    }

    // 执行任务（使用新执行器）
    let engine = core_api::ExecutionEngine::builder(ctx, &exec_opts)
        .processors(processors)
        .renderer(renderer)
        .retry_strategy(retry_strategy)
        .concurrency_strategy(concurrency_strategy)
        .build();

    let result = match engine.execute_tasks(tasks, planner).await {
        Ok(result) => {
            // 刷新STDIO事件缓冲
            core_api::flush_event_buffer();

            // 计算退出码
            let exit_code = if result.failed > 0 {
                result
                    .task_results
                    .values()
                    .find(|r| r.exit_code != 0)
                    .map(|r| r.exit_code)
                    .unwrap_or(1)
            } else {
                0
            };
            Ok(exit_code)
        }
        Err(e) => {
            // 刷新STDIO事件缓冲
            core_api::flush_event_buffer();

            // 转换ExecutorError为StdioError
            let stdio_err = core_api::StdioError::RunnerError(e.to_string());
            let code = stdio_err.error_code().as_u16() as i32;
            emit_stdio_error(&args, &stdio_err, None);
            Ok(code)
        }
    };

    result

    // ===== 新执行器调用结束 =====
}

fn load_resume_context(events_file: &str, run_id: &str) -> Result<String, String> {
    use std::io::BufRead;

    let file = std::fs::File::open(events_file)
        .map_err(|e| format!("Failed to open events file: {}", e))?;
    let reader = std::io::BufReader::new(file);

    let mut context_lines = Vec::new();
    let mut found_run = false;

    for line in reader.lines() {
        let line = line.map_err(|e| format!("Failed to read line: {}", e))?;

        // Parse JSONL
        if let Ok(ev) = serde_json::from_str::<serde_json::Value>(&line) {
            if let Some(rid) = ev.get("run_id").and_then(|v| v.as_str()) {
                if rid == run_id {
                    found_run = true;

                    // Collect relevant events
                    if let Some(event_type) = ev.get("type").and_then(|v| v.as_str()) {
                        match event_type {
                            "assistant.output" | "assistant.thinking" | "assistant.action" => {
                                if let Some(output) = ev.get("output").and_then(|v| v.as_str()) {
                                    context_lines.push(output.to_string());
                                }
                            }
                            "tool.result" => {
                                if let Some(action) = ev.get("action").and_then(|v| v.as_str()) {
                                    if let Some(output) = ev.get("output").and_then(|v| v.as_str())
                                    {
                                        context_lines
                                            .push(format!("[Tool: {}]\n{}", action, output));
                                    }
                                }
                            }
                            "run.end" => break,
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    if !found_run {
        return Err(format!("Run ID {} not found in events file", run_id));
    }

    if context_lines.is_empty() {
        return Ok(String::new());
    }

    Ok(format!(
        "=== Previous Context (run_id: {}) ===\n{}\n=== End Previous Context ===\n\n",
        run_id,
        context_lines.join("\n")
    ))
}

fn emit_stdio_error(args: &StdioArgs, err: &core_api::StdioError, run_id: Option<String>) {
    let id = run_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    if args.stream_format == "jsonl" {
        let code = err.error_code().as_u16() as i32;
        let ev = core_api::JsonlEvent {
            v: 1,
            event_type: "error".to_string(),
            ts: chrono::Local::now().to_rfc3339(),
            run_id: id,
            task_id: None,
            action: None,
            args: None,
            output: None,
            error: Some(err.to_string()),
            code: Some(code),
            progress: None,
            metadata: None,
        };
        core_api::emit_stdio_json(&ev);

        let end = core_api::JsonlEvent {
            v: 1,
            event_type: "run.end".to_string(),
            ts: chrono::Local::now().to_rfc3339(),
            run_id: ev.run_id.clone(),
            task_id: None,
            action: None,
            args: None,
            output: None,
            error: None,
            code: Some(code),
            progress: Some(100),
            metadata: Some(serde_json::json!({ "status": "failed" })),
        };
        core_api::emit_stdio_json(&end);
    } else {
        let marker = if args.ascii { "[FAIL]" } else { "✗" };
        eprintln!("{} {}", marker, err);
    }
}
