//! 引擎主入口：把一次“用户 query”编排为 pre-run（记忆检索/注入）→ runner 执行 → post-run（gatekeeper/回写）。
use std::future::Future;

use chrono::Local;

use crate::backend::BackendPlan;
use crate::error::RunnerError;
use crate::events_out::write_wrapper_event;
use crate::memory::{CandidateExtractConfig, InjectConfig, InjectPlacement};
use crate::runner::{RunnerResult, RunnerStartArgs};
use crate::tool_event::WrapperEvent;

use super::post::{post_run, PostRunContext};
use super::pre::{pre_run, EngineContext};
use super::types::{RunSessionInput, RunWithQueryArgs, RunnerSpec};

pub async fn run_with_query<F, Fut>(
    args: RunWithQueryArgs,
    run_session_fn: F,
) -> Result<i32, RunnerError>
where
    F: FnOnce(RunSessionInput) -> Fut,
    Fut: Future<Output = Result<RunnerResult, RunnerError>>,
{
    let RunWithQueryArgs {
        user_query,
        cfg,
        runner,
        run_id,
        capture_bytes,
        stream_format,
        project_id,
        events_out_tx,
        services,
        wrapper_start_data,
    } = args;

    let inject_cfg: InjectConfig = InjectConfig {
        placement: match cfg.prompt_inject.placement {
            crate::config::PromptInjectPlacement::System => InjectPlacement::System,
            crate::config::PromptInjectPlacement::User => InjectPlacement::User,
        },
        max_items: cfg.prompt_inject.max_items,
        max_answer_chars: cfg.prompt_inject.max_answer_chars,
        include_meta_line: cfg.prompt_inject.include_meta_line,
    };

    tracing::info!(
        "run_with_query: run_id={}, inject_placement={:?}, inject_max_items={}",
        run_id,
        inject_cfg.placement,
        inject_cfg.max_items
    );
    let memory = services.memory;
    let gatekeeper = services.gatekeeper;
    let policy = services.policy;

    let cand_cfg: CandidateExtractConfig = CandidateExtractConfig {
        max_candidates: cfg.candidate_extract.max_candidates,
        max_answer_chars: cfg.candidate_extract.max_answer_chars,
        min_answer_chars: cfg.candidate_extract.min_answer_chars,
        context_lines: cfg.candidate_extract.context_lines,
        tool_steps_max: cfg.candidate_extract.tool_steps_max,
        tool_step_args_keys_max: cfg.candidate_extract.tool_step_args_keys_max,
        tool_step_value_max_chars: cfg.candidate_extract.tool_step_value_max_chars,
        redact: cfg.candidate_extract.redact,
        strict_secret_block: cfg.candidate_extract.strict_secret_block,
        confidence: cfg.candidate_extract.confidence,
    };

    let (memory_search_limit, memory_min_score) = match &cfg.memory.provider {
        crate::config::MemoryProvider::Service(svc_cfg) => {
            (svc_cfg.search_limit, svc_cfg.min_score)
        }
    };

    let pre_ctx = EngineContext {
        project_id: &project_id,
        inject_cfg: &inject_cfg,
        memory: memory.as_deref(),
        gatekeeper: gatekeeper.as_ref(),
        memory_search_limit,
        memory_min_score,
    };

    let pre = pre_run(&pre_ctx, &user_query).await;
    let merged_query = pre.merged_query;
    let shown_qa_ids = pre.shown_qa_ids;
    let matches = pre.matches;
    let memory_search_event = pre.memory_search_event;

    tracing::info!(
        "run_with_query: run_id={}, merged_query_len={}, shown_qa_ids={:?}, matches_len={}",
        run_id,
        merged_query.len(),
        shown_qa_ids,
        matches.len()
    );

    // Buffer early wrapper events until we learn the effective run_id.
    // Note: Some backends (e.g., Gemini) return "session_id" which is treated as run_id.
    // This keeps IDs consistent across the whole wrapper-event stream.
    let mut pending_wrapper_events: Vec<WrapperEvent> = Vec::new();
    if let Some(ev) = memory_search_event {
        pending_wrapper_events.push(ev);
    }

    let mut start_event = WrapperEvent::new("run.start", Local::now().to_rfc3339());
    start_event.data = wrapper_start_data;
    pending_wrapper_events.push(start_event);

    // Build runner + session args (backend plan runs after memory injection)
    let (runner, session_args) = build_runner_and_args(runner, merged_query)?;

    tracing::info!("Starting runner '{}' for run_id={}", runner.name(), run_id);

    // Always include the actual backend invocation in wrapper events for replay/observability.
    if let Some(last) = pending_wrapper_events.last_mut() {
        match last.data.as_mut() {
            Some(serde_json::Value::Object(map)) => {
                map.entry("cmd".to_string())
                    .or_insert_with(|| serde_json::Value::String(session_args.cmd.clone()));
                map.entry("args".to_string())
                    .or_insert_with(|| serde_json::json!(session_args.args.clone()));
            }
            None => {
                last.data = Some(serde_json::json!({
                    "cmd": session_args.cmd.clone(),
                    "args": session_args.args.clone(),
                }));
            }
            Some(_) => {
                last.data = Some(serde_json::json!({
                    "cmd": session_args.cmd.clone(),
                    "args": session_args.args.clone(),
                }));
            }
        }
    }
    let stdin_payload = session_args.stdin_payload.clone();
    // Start Session
    let session = match runner.start_session(&session_args).await {
        Ok(session) => session,
        Err(e) => {
            // Best-effort: still emit buffered wrapper events so the run has a trace,
            // using the configured run_id (no session_id discovered).
            tracing::error!(
                "runner '{}' failed to start session for run_id={}: {}",
                runner.name(),
                run_id,
                e
            );
            for mut ev in pending_wrapper_events {
                ev.run_id = Some(run_id.clone());
                write_wrapper_event(events_out_tx.as_ref(), &ev).await;
            }
            return Err(RunnerError::Spawn(e.to_string()));
        }
    };

    let run_input = RunSessionInput {
        session,
        run_id: run_id.clone(),
        control: cfg.control.clone(),
        policy,
        capture_bytes,
        events_out_tx: events_out_tx.clone(),
        backend_kind: cfg.backend_kind,
        stream_format: stream_format.clone(),
        stdin_payload,
    };

    // Run Session (runner runtime is in core; caller may provide a custom session loop, e.g. TUI).
    let run_result = match run_session_fn(run_input).await {
        Ok(r) => r,
        Err(e) => {
            // Best-effort: still emit buffered wrapper events so the run has a trace,
            // using the configured run_id (no session_id discovered).
            for mut ev in pending_wrapper_events {
                ev.run_id = Some(run_id.clone());
                write_wrapper_event(events_out_tx.as_ref(), &ev).await;
            }
            return Err(e);
        }
    };

    let effective_run_id = run_result.run_id.clone();

    // Flush buffered wrapper events with a consistent run_id.
    for mut ev in pending_wrapper_events {
        ev.run_id = Some(effective_run_id.clone());
        write_wrapper_event(events_out_tx.as_ref(), &ev).await;
    }

    if run_result.dropped_lines > 0 {
        let mut ev = WrapperEvent::new("tee.drop", Local::now().to_rfc3339());
        ev.run_id = Some(effective_run_id.clone());
        ev.data = Some(serde_json::json!({ "dropped_lines": run_result.dropped_lines }));
        write_wrapper_event(events_out_tx.as_ref(), &ev).await;
    }

    let post_ctx = PostRunContext {
        project_id: &project_id,
        cand_cfg: &cand_cfg,
        memory: memory.as_deref(),
        gatekeeper: gatekeeper.as_ref(),
        events_out: events_out_tx.as_ref(),
    };

    let (run_outcome, _decision) =
        post_run(&post_ctx, &run_result, &matches, shown_qa_ids, &user_query).await?;

    let mut exit_event = WrapperEvent::new("run.end", Local::now().to_rfc3339());
    exit_event.run_id = Some(effective_run_id);
    exit_event.data = Some(serde_json::json!({
        "exit_code": run_outcome.exit_code,
        "duration_ms": run_outcome.duration_ms,
        "stdout_tail": run_outcome.stdout_tail,
        "stderr_tail": run_outcome.stderr_tail,
        "used_qa_ids": run_outcome.used_qa_ids,
        "shown_qa_ids": run_outcome.shown_qa_ids,
    }));
    write_wrapper_event(events_out_tx.as_ref(), &exit_event).await;
    tracing::info!(
        "run completed: run_id={}, exit_code={}",
        run_id,
        run_outcome.exit_code
    );
    Ok(run_outcome.exit_code)
}

fn build_runner_and_args(
    runner: RunnerSpec,
    merged_query: String,
) -> Result<(Box<dyn crate::runner::RunnerPlugin>, RunnerStartArgs), RunnerError> {
    match runner {
        RunnerSpec::Backend {
            strategy,
            backend_spec,
            base_envs,
            resume_id,
            model,
            model_provider,
            project_id,
            stream_format,
        } => {
            let request = crate::backend::BackendPlanRequest {
                backend: backend_spec,
                base_envs,
                resume_id,
                prompt: merged_query,
                model,
                model_provider,
                project_id,
                stream_format,
            };

            let BackendPlan {
                runner,
                session_args,
            } = strategy
                .plan(request)
                .map_err(|e| RunnerError::Spawn(e.to_string()))?;
            Ok((runner, session_args))
        }
        RunnerSpec::Passthrough {
            runner,
            session_args,
        } => Ok((runner, session_args)),
    }
}
