use memex_core::api as core_api;
use memex_plugins::factory;
use memex_plugins::plan::{build_runner_spec, PlanMode, PlanRequest};
use tokio::sync::mpsc;

pub async fn execute_stdio_tasks(
    tasks: &Vec<core_api::StdioTask>,
    ctx: &core_api::AppContext,
    stdio_opts: &core_api::StdioRunOpts,
    http_sse_tx: Option<mpsc::UnboundedSender<Vec<u8>>>,
) -> Result<core_api::ExecutionResult, core_api::ExecutorError> {
    core_api::configure_event_buffer(
        ctx.cfg().stdio.enable_event_buffering,
        ctx.cfg().stdio.event_buffer_size,
        ctx.cfg().stdio.event_flush_interval_ms,
    );

    let mut exec_opts = core_api::ExecutionOpts::from_stdio_config(stdio_opts, &ctx.cfg().stdio);
    exec_opts.http_sse_tx = http_sse_tx;

    let cfg_for_planner = ctx.cfg().clone();
    let planner = move |task: &core_api::StdioTask| -> Result<
        (core_api::RunnerSpec, Option<serde_json::Value>),
        core_api::StdioError,
    > {
        let mut cfg = cfg_for_planner.clone();
        let plan_req = PlanRequest {
            mode: PlanMode::Backend {
                backend_spec: task.backend.clone(),
                backend_kind: task.backend_kind.as_ref().map(|k| *k),
                env_file: task.env_file.clone(),
                env: task.env.clone().unwrap_or_default(),
                model: task.model.clone(),
                model_provider: task.model_provider.clone(),
                project_id: Some(task.workdir.clone()),
                task_level: task.task_level.clone(),
            },
            resume_id: task.resume_run_id.clone(),
            stream_format: task.stream_format.clone(),
        };
        let (runner_spec,) = build_runner_spec(&mut cfg, plan_req)
            .map_err(|e| core_api::StdioError::BackendError(e.to_string()))?;
        Ok((runner_spec, None))
    };

    let processors = factory::build_task_processors(&ctx.cfg().executor);
    let renderer = factory::build_renderer(&stdio_opts.stream_format, &ctx.cfg().executor.output);
    let retry_strategy = factory::build_retry_strategy(&ctx.cfg().executor.retry);
    let concurrency_strategy = factory::build_concurrency_strategy(&ctx.cfg().executor.concurrency);

    let engine = core_api::ExecutionEngine::builder(ctx, &exec_opts)
        .processors(processors)
        .renderer(renderer)
        .retry_strategy(retry_strategy)
        .concurrency_strategy(concurrency_strategy)
        .build();

    let result = engine.execute_tasks(tasks, planner).await;

    core_api::flush_event_buffer();

    result
}
