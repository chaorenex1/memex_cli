use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use uuid::Uuid;

use crate::context::AppContext;
use crate::engine::run_with_query;
use crate::error::ExecutorError;
use crate::runner::{run_session, RunSessionArgs, RunnerResult};
use crate::stdio::StdioTask;

use super::graph::TaskGraph;
use super::output::{
    emit_execution_plan, emit_run_end, emit_run_start, emit_stage_end, emit_stage_start,
};
use super::progress::ProgressMonitor;
use super::traits::{
    ConcurrencyContext, ConcurrencyStrategyPlugin, DependencyResult, OutputRendererPlugin,
    ProcessContext, RenderEvent, RetryStrategyPlugin, TaskProcessorPlugin,
};
use super::types::{ExecutionOpts, ExecutionResult, TaskResult};

struct SystemInfoCache {
    cpu_count: usize,
    last_refresh: Instant,
    cached_cpu_usage: f32,
    cached_memory_usage: f32,
}

impl SystemInfoCache {
    fn new() -> Self {
        let mut sys = sysinfo::System::new();
        sys.refresh_cpu();
        sys.refresh_memory();
        let cpu_count = sys.cpus().len().max(1);
        let cpu_usage = sys.cpus().iter().map(|c| c.cpu_usage()).sum::<f32>() / cpu_count as f32;
        let total_memory = sys.total_memory().max(1);
        let memory_usage = (sys.used_memory() as f32 / total_memory as f32) * 100.0;

        Self {
            cpu_count,
            last_refresh: Instant::now(),
            cached_cpu_usage: cpu_usage,
            cached_memory_usage: memory_usage,
        }
    }

    fn get(&mut self) -> (usize, f32, f32) {
        if self.last_refresh.elapsed() > Duration::from_secs(1) {
            // Reuse single System instance instead of creating new one - significant CPU savings
            // This avoids the overhead of re-detecting system hardware on each refresh
            static mut SYS: Option<sysinfo::System> = None;
            static INIT: std::sync::Once = std::sync::Once::new();

            unsafe {
                INIT.call_once(|| {
                    SYS = Some(sysinfo::System::new());
                });
                if let Some(ref mut sys) = SYS {
                    sys.refresh_cpu();
                    sys.refresh_memory();
                    self.cached_cpu_usage = sys.cpus().iter().map(|c| c.cpu_usage()).sum::<f32>()
                        / self.cpu_count as f32;
                    let total_memory = sys.total_memory().max(1);
                    self.cached_memory_usage =
                        (sys.used_memory() as f32 / total_memory as f32) * 100.0;
                }
            }
            self.last_refresh = Instant::now();
        }
        (
            self.cpu_count,
            self.cached_cpu_usage,
            self.cached_memory_usage,
        )
    }
}

/// Execution engine for task dependency graphs
pub struct ExecutionEngine<'a> {
    ctx: &'a AppContext,
    opts: &'a ExecutionOpts,
    processors: Vec<Arc<dyn TaskProcessorPlugin>>,
    renderer: Option<Arc<dyn OutputRendererPlugin>>,
    retry_strategy: Option<Arc<dyn RetryStrategyPlugin>>,
    concurrency_strategy: Option<Arc<dyn ConcurrencyStrategyPlugin>>,
    sys_cache: Mutex<SystemInfoCache>,
}

pub struct ExecutionEngineBuilder<'a> {
    ctx: &'a AppContext,
    opts: &'a ExecutionOpts,
    processors: Vec<Arc<dyn TaskProcessorPlugin>>,
    renderer: Option<Arc<dyn OutputRendererPlugin>>,
    retry_strategy: Option<Arc<dyn RetryStrategyPlugin>>,
    concurrency_strategy: Option<Arc<dyn ConcurrencyStrategyPlugin>>,
    sys_cache: Mutex<SystemInfoCache>,
}

impl<'a> ExecutionEngine<'a> {
    pub fn new(ctx: &'a AppContext, opts: &'a ExecutionOpts) -> Self {
        Self {
            ctx,
            opts,
            processors: Vec::new(),
            renderer: None,
            retry_strategy: None,
            concurrency_strategy: None,
            sys_cache: Mutex::new(SystemInfoCache::new()),
        }
    }

    pub fn builder(ctx: &'a AppContext, opts: &'a ExecutionOpts) -> ExecutionEngineBuilder<'a> {
        ExecutionEngineBuilder::new(ctx, opts)
    }

    /// Execute tasks with dependency graph support using injected plugins.
    pub async fn execute_tasks<F>(
        &self,
        tasks: &Vec<StdioTask>,
        planner: F,
    ) -> Result<ExecutionResult, ExecutorError>
    where
        F: Fn(
                &StdioTask,
            ) -> Result<
                (crate::api::RunnerSpec, Option<serde_json::Value>),
                crate::stdio::StdioError,
            > + Clone
            + Send
            + Sync
            + 'static,
    {
        let run_id = tasks
            .first()
            .and_then(|t| {
                if !t.id.is_empty() {
                    Some(t.id.clone())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        let graph = TaskGraph::from_tasks(tasks)?;
        graph.validate()?;
        let stages = graph.topological_sort()?;

        self.emit_run_start(&run_id, graph.nodes.len(), stages.len());

        let result = self
            .execute_stages(stages, &graph, &run_id, planner)
            .await?;

        self.emit_run_end(&run_id, &result);

        Ok(result)
    }
    /// Execute all stages sequentially (tasks within a stage run in parallel)
    pub async fn execute_stages<F>(
        &self,
        stages: Vec<Vec<String>>,
        graph: &TaskGraph<StdioTask>,
        run_id: &str,
        planner: F,
    ) -> Result<ExecutionResult, ExecutorError>
    where
        F: Fn(
                &StdioTask,
            ) -> Result<
                (crate::api::RunnerSpec, Option<serde_json::Value>),
                crate::stdio::StdioError,
            > + Clone
            + Send
            + Sync
            + 'static,
    {
        let start = Instant::now();
        let mut task_results = HashMap::new();
        let total_tasks = graph.nodes.len();
        let total_stages = stages.len();

        // Create progress monitor (enabled based on opts)
        let progress = Arc::new(Mutex::new(ProgressMonitor::new(
            total_tasks,
            self.opts.progress_bar,
        )));

        // Emit execution plan
        self.emit_plan(run_id, &stages);

        // Execute each stage sequentially
        for (stage_id, task_ids) in stages.iter().enumerate() {
            self.emit_stage_start(run_id, stage_id, task_ids);

            // Update progress monitor stage
            if let Ok(monitor) = progress.lock() {
                monitor.update_stage(stage_id, total_stages);
            }

            // Execute this stage's tasks in parallel
            let stage_results = self
                .execute_stage_tasks(
                    stage_id,
                    task_ids,
                    graph,
                    &task_results,
                    run_id,
                    planner.clone(),
                    progress.clone(),
                )
                .await?;

            task_results.extend(stage_results);

            self.emit_stage_end(run_id, stage_id);

            // Emit progress update after each stage
            self.emit_progress_update(
                run_id,
                task_results.len(),
                total_tasks,
                stage_id,
                total_stages,
            );

            // Stop on first failure (fail-fast)
            if task_results.values().any(|r| r.exit_code != 0) {
                break;
            }
        }

        // Finish progress monitor
        let all_success = task_results.values().all(|r| r.exit_code == 0);
        if let Ok(monitor) = progress.lock() {
            monitor.finish(all_success);
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        let failed = task_results.values().filter(|r| r.exit_code != 0).count();

        Ok(ExecutionResult {
            total_tasks,
            completed: task_results.len(),
            failed,
            duration_ms,
            task_results,
            stages,
        })
    }

    /// Execute tasks in a single stage (in parallel)
    async fn execute_stage_tasks<F>(
        &self,
        stage_id: usize,
        task_ids: &[String],
        graph: &TaskGraph<StdioTask>,
        prev_results: &HashMap<String, TaskResult>,
        run_id: &str,
        planner: F,
        progress: Arc<Mutex<ProgressMonitor>>,
    ) -> Result<HashMap<String, TaskResult>, ExecutorError>
    where
        F: Fn(
                &StdioTask,
            ) -> Result<
                (crate::api::RunnerSpec, Option<serde_json::Value>),
                crate::stdio::StdioError,
            > + Clone
            + Send
            + Sync
            + 'static,
    {
        use crate::stdio::StdioRunOpts;
        use std::sync::Arc;

        // Determine maximum concurrency
        let base_parallel = self
            .opts
            .max_parallel
            .unwrap_or_else(|| self.ctx.cfg().stdio.max_parallel_tasks);
        let max_parallel = self
            .concurrency_strategy
            .as_ref()
            .map(|strategy| {
                let context = build_concurrency_context(
                    self.ctx,
                    base_parallel,
                    task_ids.len(),
                    &self.sys_cache,
                );
                strategy.calculate_concurrency(&context)
            })
            .unwrap_or(base_parallel)
            .max(1);

        // Convert ExecutionOpts to StdioRunOpts
        let stdio_opts = StdioRunOpts {
            stream_format: self.opts.stream_format.clone(),
            capture_bytes: self.opts.capture_bytes,
            verbose: self.opts.verbose,
            quiet: self.opts.quiet,
            ascii: self.opts.ascii,
            resume_run_id: self.opts.resume_run_id.clone(),
            resume_context: self.opts.resume_context.clone(),
        };

        // Clone context for parallel execution
        let ctx = self.ctx.clone();
        let graph_clone = graph.clone();
        let prev_results_clone = prev_results.clone();
        let run_id_owned = run_id.to_string();
        let stdio_opts_arc = Arc::new(stdio_opts);
        let exec_opts = self.opts.clone();
        let renderer = self.renderer.clone();
        let processors = self.processors.clone();
        let app_config = Arc::new(self.ctx.cfg().clone());
        let has_context_injector = processors
            .iter()
            .any(|processor| processor.name() == "context-injector");
        let retry_strategy = self.retry_strategy.clone();

        // Build services from context
        let services = Arc::new(
            self.ctx
                .build_services(self.ctx.cfg())
                .await
                .map_err(|e| ExecutorError::Runner(e.to_string()))?,
        );

        // Add all tasks to progress monitor
        if let Ok(mut monitor) = progress.lock() {
            for task_id in task_ids {
                monitor.add_task(task_id);
            }
        }

        // Create executor function for parallel execution
        let executor_fn = move |task_id: String| {
            let ctx = ctx.clone();
            let graph = graph_clone.clone();
            let prev_results = prev_results_clone.clone();
            let run_id = run_id_owned.clone();
            let stdio_opts = stdio_opts_arc.clone();
            let services = services.clone();
            let planner = planner.clone();
            let opts = exec_opts.clone();
            let progress = progress.clone();
            let renderer = renderer.clone();
            let processors = processors.clone();
            let app_config = app_config.clone();
            let retry_strategy = retry_strategy.clone();

            async move {
                // Get task from graph
                let task = graph
                    .nodes
                    .get(&task_id)
                    .ok_or_else(|| ExecutorError::Runner(format!("Task not found: {}", task_id)))?
                    .clone();

                // Emit task start event
                emit_task_start(&opts, &run_id, &task_id, stage_id, &renderer);

                // Build dependency context
                let (dependency_outputs, dependency_results) =
                    build_dependency_results(&task, &prev_results);

                let dep_context_opt = if has_context_injector {
                    None
                } else {
                    let dep_context = build_dependency_context(&task, &prev_results);
                    if dep_context.is_empty() {
                        None
                    } else {
                        Some(dep_context)
                    }
                };

                // Apply processors (if any) to build enhanced content
                let mut exec_task = task.to_executable_task();
                if !processors.is_empty() {
                    let process_ctx = ProcessContext {
                        dependency_outputs,
                        dependency_results,
                        run_id: run_id.clone(),
                        stage_id,
                        app_config: app_config.clone(),
                    };

                    for processor in &processors {
                        let processed = processor
                            .process(&exec_task, &process_ctx)
                            .await
                            .map_err(|e| ExecutorError::Runner(e.to_string()))?;
                        exec_task.content = processed.enhanced_content;
                    }
                }

                let mut task_to_run = task.clone();
                task_to_run.content = exec_task.content;

                // Execute task using the injected planner (with optional retry strategy)
                let max_attempts = retry_strategy
                    .as_ref()
                    .map(|strategy| strategy.max_attempts().max(1))
                    .unwrap_or(1);

                // First attempt
                let mut current = execute_task_once(
                    {
                        let mut t = task_to_run.clone();
                        if retry_strategy.is_some() {
                            t.retry = Some(0);
                        }
                        t
                    },
                    &ctx,
                    &opts,
                    &stdio_opts,
                    planner.clone(),
                    services.clone(),
                    &run_id,
                    dep_context_opt.clone(),
                )
                .await?;

                // Retry if needed
                let mut retries_used: u32 = 0;
                if current.exit_code != 0 {
                    if let Some(strategy) = &retry_strategy {
                        for attempt in 1..max_attempts {
                            let err = format!("exit_code: {}", current.exit_code);
                            if !strategy.should_retry(attempt, &err) {
                                break;
                            }

                            let Some(delay) = strategy.next_delay(attempt, &err) else {
                                break;
                            };

                            tokio::time::sleep(delay).await;

                            let retry_outcome = execute_task_once(
                                {
                                    let mut t = task_to_run.clone();
                                    t.retry = Some(attempt);
                                    t
                                },
                                &ctx,
                                &opts,
                                &stdio_opts,
                                planner.clone(),
                                services.clone(),
                                &run_id,
                                dep_context_opt.clone(),
                            )
                            .await?;

                            current.duration_ms = current
                                .duration_ms
                                .saturating_add(retry_outcome.duration_ms);
                            current.exit_code = retry_outcome.exit_code;
                            current.output = retry_outcome.output;
                            retries_used = attempt;

                            if current.exit_code == 0 {
                                break;
                            }
                        }
                    }
                }

                let total_duration_ms = current.duration_ms;
                let final_exit_code = current.exit_code;
                let final_output = current.output;

                emit_task_complete(
                    &opts,
                    &run_id,
                    &task_id,
                    final_exit_code,
                    total_duration_ms,
                    retries_used,
                    &renderer,
                );

                // Update progress monitor
                if let Ok(mut monitor) = progress.lock() {
                    monitor.complete_task(&task_id, final_exit_code == 0, total_duration_ms);
                }

                // Build result
                Ok(TaskResult {
                    task_id: task_id.clone(),
                    exit_code: final_exit_code,
                    duration_ms: total_duration_ms,
                    output: final_output,
                    error: if final_exit_code != 0 {
                        Some(format!("Task failed with exit code {}", final_exit_code))
                    } else {
                        None
                    },
                    retries_used,
                })
            }
        };

        // Execute tasks in parallel using scheduler
        let results =
            super::scheduler::execute_stage_parallel(task_ids, graph, max_parallel, executor_fn)
                .await?;

        Ok(results)
    }

    fn emit_plan(&self, run_id: &str, stages: &[Vec<String>]) {
        if let Some(renderer) = &self.renderer {
            renderer.render(&RenderEvent::Plan {
                run_id: run_id.to_string(),
                stages: stages.to_vec(),
            });
        } else {
            emit_execution_plan(self.opts, run_id, stages);
        }
    }

    fn emit_run_start(&self, run_id: &str, total_tasks: usize, total_stages: usize) {
        if let Some(renderer) = &self.renderer {
            renderer.render(&RenderEvent::RunStart {
                run_id: run_id.to_string(),
                total_tasks,
                total_stages,
            });
        } else {
            emit_run_start(self.opts, run_id, total_tasks, total_stages);
        }
    }

    fn emit_run_end(&self, run_id: &str, result: &ExecutionResult) {
        if let Some(renderer) = &self.renderer {
            renderer.render(&RenderEvent::RunEnd {
                run_id: run_id.to_string(),
                result: result.clone(),
            });
        } else {
            emit_run_end(self.opts, run_id, result);
        }
    }

    fn emit_stage_start(&self, run_id: &str, stage_id: usize, task_ids: &[String]) {
        if let Some(renderer) = &self.renderer {
            renderer.render(&RenderEvent::StageStart {
                run_id: run_id.to_string(),
                stage_id,
                task_ids: task_ids.to_vec(),
            });
        } else {
            emit_stage_start(self.opts, run_id, stage_id, task_ids);
        }
    }

    fn emit_stage_end(&self, run_id: &str, stage_id: usize) {
        if let Some(renderer) = &self.renderer {
            renderer.render(&RenderEvent::StageEnd {
                run_id: run_id.to_string(),
                stage_id,
            });
        } else {
            emit_stage_end(self.opts, run_id, stage_id);
        }
    }

    fn emit_progress_update(
        &self,
        run_id: &str,
        completed: usize,
        total: usize,
        stage_id: usize,
        total_stages: usize,
    ) {
        if let Some(renderer) = &self.renderer {
            let progress = if total > 0 {
                completed as f32 / total as f32
            } else {
                0.0
            };
            renderer.render(&RenderEvent::TaskProgress {
                run_id: run_id.to_string(),
                task_id: "executor".to_string(),
                progress,
                message: Some(format!("stage {}/{}", stage_id + 1, total_stages)),
            });
        } else {
            super::output::emit_progress_update(
                self.opts,
                run_id,
                completed,
                total,
                stage_id,
                total_stages,
            );
        }
    }
}

impl<'a> ExecutionEngineBuilder<'a> {
    pub fn new(ctx: &'a AppContext, opts: &'a ExecutionOpts) -> Self {
        Self {
            ctx,
            opts,
            processors: Vec::new(),
            renderer: None,
            retry_strategy: None,
            concurrency_strategy: None,
            sys_cache: Mutex::new(SystemInfoCache::new()),
        }
    }

    pub fn processors(mut self, processors: Vec<Arc<dyn TaskProcessorPlugin>>) -> Self {
        let mut sorted = processors;
        sorted.sort_by_key(|p| std::cmp::Reverse(p.priority()));
        self.processors = sorted;
        self
    }

    pub fn renderer(mut self, renderer: Arc<dyn OutputRendererPlugin>) -> Self {
        self.renderer = Some(renderer);
        self
    }

    pub fn retry_strategy(mut self, strategy: Arc<dyn RetryStrategyPlugin>) -> Self {
        self.retry_strategy = Some(strategy);
        self
    }

    pub fn concurrency_strategy(mut self, strategy: Arc<dyn ConcurrencyStrategyPlugin>) -> Self {
        self.concurrency_strategy = Some(strategy);
        self
    }

    pub fn build(self) -> ExecutionEngine<'a> {
        ExecutionEngine {
            ctx: self.ctx,
            opts: self.opts,
            processors: self.processors,
            renderer: self.renderer,
            retry_strategy: self.retry_strategy,
            concurrency_strategy: self.concurrency_strategy,
            sys_cache: self.sys_cache,
        }
    }
}

/// Build dependency context from previous task results
fn build_dependency_context(
    task: &StdioTask,
    prev_results: &HashMap<String, TaskResult>,
) -> String {
    if task.dependencies.is_empty() {
        return String::new();
    }

    use std::fmt::Write;

    let estimated_size = task.dependencies.len() * 200 + 50;
    let mut context = String::with_capacity(estimated_size);
    context.push_str("=== Dependency Outputs ===\n\n");

    for dep_id in &task.dependencies {
        if let Some(result) = prev_results.get(dep_id) {
            context.push_str("# Task: ");
            context.push_str(dep_id);
            context.push_str("\nExit Code: ");
            let _ = writeln!(context, "{}", result.exit_code);
            if !result.output.is_empty() {
                context.push_str("Output:\n");
                context.push_str(&result.output);
                context.push_str("\n\n");
            }
        }
    }

    context.push_str("=== End Dependency Outputs ===\n");
    context
}

/// Execute tasks with dependency graph support
///
/// This is the main entry point for the executor module.
///
/// # Arguments
///
/// * `tasks` - List of tasks to execute
/// * `ctx` - Application context
/// * `opts` - Execution options
/// * `planner` - Function to create RunnerSpec from task metadata
///
/// # Returns
///
/// Detailed execution result including per-task status
pub async fn execute_tasks<F>(
    tasks: &Vec<StdioTask>,
    ctx: &AppContext,
    opts: &ExecutionOpts,
    planner: F,
) -> Result<ExecutionResult, ExecutorError>
where
    F: Fn(
            &StdioTask,
        )
            -> Result<(crate::api::RunnerSpec, Option<serde_json::Value>), crate::stdio::StdioError>
        + Clone
        + Send
        + Sync
        + 'static,
{
    let engine = ExecutionEngine::new(ctx, opts);
    engine.execute_tasks(tasks, planner).await
}

fn emit_task_start(
    opts: &ExecutionOpts,
    run_id: &str,
    task_id: &str,
    stage_id: usize,
    renderer: &Option<Arc<dyn OutputRendererPlugin>>,
) {
    if let Some(renderer) = renderer {
        renderer.render(&RenderEvent::TaskStart {
            run_id: run_id.to_string(),
            task_id: task_id.to_string(),
            stage_id,
        });
    } else {
        super::output::emit_task_start(opts, run_id, task_id, stage_id);
    }
}

fn emit_task_complete(
    opts: &ExecutionOpts,
    run_id: &str,
    task_id: &str,
    exit_code: i32,
    duration_ms: u64,
    retries_used: u32,
    renderer: &Option<Arc<dyn OutputRendererPlugin>>,
) {
    if let Some(renderer) = renderer {
        renderer.render(&RenderEvent::TaskComplete {
            run_id: run_id.to_string(),
            task_id: task_id.to_string(),
            result: TaskResult {
                task_id: task_id.to_string(),
                exit_code,
                duration_ms,
                output: String::new(),
                error: None,
                retries_used,
            },
        });
    } else {
        super::output::emit_task_complete(
            opts,
            run_id,
            task_id,
            exit_code,
            duration_ms,
            retries_used,
        );
    }
}

fn build_dependency_results(
    task: &StdioTask,
    prev_results: &HashMap<String, TaskResult>,
) -> (HashMap<String, String>, HashMap<String, DependencyResult>) {
    let mut outputs = HashMap::new();
    let mut results = HashMap::new();

    for dep_id in &task.dependencies {
        if let Some(result) = prev_results.get(dep_id) {
            outputs.insert(dep_id.clone(), result.output.clone());
            results.insert(
                dep_id.clone(),
                DependencyResult {
                    exit_code: result.exit_code,
                    output: result.output.clone(),
                },
            );
        }
    }

    (outputs, results)
}

fn build_concurrency_context(
    ctx: &AppContext,
    base_concurrency: usize,
    active_tasks: usize,
    sys_cache: &Mutex<SystemInfoCache>,
) -> ConcurrencyContext {
    let mut cache = match sys_cache.lock() {
        Ok(cache) => cache,
        Err(poisoned) => poisoned.into_inner(),
    };
    let (cpu_count, cpu_usage, memory_usage) = cache.get();

    ConcurrencyContext {
        cpu_usage,
        available_cpus: cpu_count,
        memory_usage,
        active_tasks,
        base_concurrency: ctx
            .cfg()
            .executor
            .concurrency
            .base_concurrency
            .max(base_concurrency),
    }
}

#[derive(Debug, Clone)]
struct TaskRunOutput {
    exit_code: i32,
    output: String,
    duration_ms: u64,
}

fn apply_dependency_context(content: &str, dep_context: &Option<String>) -> String {
    let Some(ctx) = dep_context.as_ref() else {
        return content.to_string();
    };
    if ctx.is_empty() {
        return content.to_string();
    }
    if ctx.ends_with('\n') {
        format!("{ctx}{content}")
    } else {
        format!("{ctx}\n\n{content}")
    }
}

fn append_output_line(target: &mut String, line: &str) {
    if line.is_empty() {
        return;
    }
    target.push_str(line);
    if !target.ends_with('\n') {
        target.push('\n');
    }
}

fn extract_output_from_runner_result(result: &RunnerResult) -> String {
    if result.tool_events.is_empty() {
        return result.stdout_tail.clone();
    }

    let mut out = String::new();
    for ev in &result.tool_events {
        match ev.event_type.as_str() {
            "assistant.output" | "assistant.thinking" | "assistant.action" => {
                if let Some(text) = ev.output.as_ref().and_then(|v| v.as_str()) {
                    append_output_line(&mut out, text);
                }
            }
            "tool.result" => {
                if let Some(action) = ev.action.as_ref() {
                    if let Some(text) = ev.output.as_ref().and_then(|v| v.as_str()) {
                        let block = format!("[Tool: {action}]\n{text}");
                        append_output_line(&mut out, &block);
                    }
                }
            }
            _ => {}
        }
    }

    if out.is_empty() {
        result.stdout_tail.clone()
    } else {
        out
    }
}

async fn execute_task_once<F>(
    task: StdioTask,
    ctx: &AppContext,
    exec_opts: &ExecutionOpts,
    opts: &crate::stdio::StdioRunOpts,
    planner: F,
    services: Arc<crate::context::Services>,
    run_id: &str,
    dep_context: Option<String>,
) -> Result<TaskRunOutput, ExecutorError>
where
    F: Fn(
            &StdioTask,
        )
            -> Result<(crate::api::RunnerSpec, Option<serde_json::Value>), crate::stdio::StdioError>
        + Clone
        + Send
        + Sync
        + 'static,
{
    let prompt = apply_dependency_context(&task.content, &dep_context);
    let (runner_spec, start_data) =
        planner(&task).map_err(|e| ExecutorError::Runner(e.to_string()))?;

    let run_args = crate::engine::RunWithQueryArgs {
        user_query: prompt,
        cfg: ctx.cfg().clone(),
        runner: runner_spec,
        run_id: run_id.to_string(),
        capture_bytes: opts.capture_bytes,
        stream_format: task.stream_format.clone(),
        project_id: crate::util::generate_project_id_str(&task.workdir),
        events_out_tx: ctx.events_out(),
        services: services.as_ref().clone(),
        wrapper_start_data: start_data,
    };

    let result_holder: Arc<Mutex<Option<RunnerResult>>> = Arc::new(Mutex::new(None));
    let result_holder_clone = result_holder.clone();
    let timeout_secs = crate::stdio::effective_timeout_secs(task.timeout);
    let (abort_tx, abort_rx) = tokio::sync::mpsc::channel::<String>(1);
    let http_sse_tx = exec_opts.http_sse_tx.clone();

    let run_fut = run_with_query(run_args, move |input| {
        let result_holder = result_holder_clone.clone();
        let http_sse_tx = http_sse_tx.clone();
        async move {
            let backend_kind = input.backend_kind.to_string();
            let parser_kind = crate::runner::ParserKind::from_stream_format(
                &input.stream_format,
                input.events_out_tx.clone(),
                &input.run_id,
            );
            let sink_kind = crate::runner::SinkKind::from_channels(http_sse_tx, None);
            let result = run_session(RunSessionArgs {
                session: input.session,
                control: &input.control,
                policy: input.policy,
                capture_bytes: input.capture_bytes,
                events_out: input.events_out_tx,
                run_id: &input.run_id,
                backend_kind: &backend_kind,
                parser_kind,
                sink_kind,
                abort_rx: Some(abort_rx),
                stdin_payload: input.stdin_payload.clone(),
            })
            .await?;

            if let Ok(mut guard) = result_holder.lock() {
                *guard = Some(result.clone());
            }

            Ok(result)
        }
    });

    tokio::pin!(run_fut);
    let timed = tokio::time::timeout(Duration::from_secs(timeout_secs), &mut run_fut).await;
    let (timed_out, run_res) = match timed {
        Ok(res) => (false, res),
        Err(_) => {
            let _ = abort_tx
                .send(format!("timeout after {}s", timeout_secs))
                .await;
            (true, run_fut.await)
        }
    };

    let exit_code = match run_res {
        Ok(code) => {
            if timed_out {
                crate::stdio::exit_code_for_timeout()
            } else {
                code
            }
        }
        Err(e) => {
            if timed_out {
                crate::stdio::exit_code_for_timeout()
            } else {
                return Err(ExecutorError::Runner(e.to_string()));
            }
        }
    };

    let (output, duration_ms) = match result_holder.lock() {
        Ok(mut guard) => {
            if let Some(result) = guard.take() {
                (
                    extract_output_from_runner_result(&result),
                    result.duration_ms.unwrap_or(0),
                )
            } else {
                (String::new(), 0)
            }
        }
        Err(_) => (String::new(), 0),
    };

    Ok(TaskRunOutput {
        exit_code,
        output,
        duration_ms,
    })
}
