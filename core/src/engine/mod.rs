use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use chrono::Utc;

use crate::backend::{BackendPlan, BackendStrategy};
use crate::config::AppConfig;
use crate::error::RunnerError;
use crate::events_out::{write_wrapper_event, EventsOutTx};
use crate::gatekeeper::{GatekeeperDecision, GatekeeperPlugin, SearchMatch};
use crate::memory::CandidateExtractConfig;
use crate::memory::InjectPlacement;
use crate::memory::MemoryPlugin;
use crate::memory::{
    build_candidate_payloads, build_hit_payload, build_validate_payloads, merge_prompt,
    render_memory_context, CandidateDraft, InjectConfig, QASearchPayload,
};
use crate::runner::{PolicyPlugin, RunOutcome, RunnerPlugin, RunnerResult, RunnerSession, RunnerStartArgs};
use crate::state::types::{GatekeeperDecisionSnapshot, RuntimePhase};
use crate::state::{StateManager, StateManagerHandle};
use crate::tool_event::{ToolEventLite, WrapperEvent};

pub struct RunSessionInput {
    pub session: Box<dyn RunnerSession>,
    pub run_id: String,
    pub control: crate::config::ControlConfig,
    pub policy: Option<Box<dyn PolicyPlugin>>,
    pub capture_bytes: usize,
    pub events_out_tx: Option<EventsOutTx>,
    pub silent: bool,
    pub state_manager: Option<Arc<StateManager>>,
    pub state_session_id: Option<String>,
}

pub enum RunnerSpec {
    Backend {
        strategy: Box<dyn BackendStrategy>,
        backend_spec: String,
        base_envs: HashMap<String, String>,
        resume_id: Option<String>,
        model: Option<String>,
        stream: bool,
        stream_format: String,
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
    pub silent: bool,
    pub events_out_tx: Option<EventsOutTx>,
    pub state_manager: Option<Arc<StateManager>>,
    pub policy: Option<Box<dyn PolicyPlugin>>,
    pub memory: Option<Box<dyn MemoryPlugin>>,
    pub gatekeeper: Box<dyn GatekeeperPlugin>,
    pub wrapper_start_data: Option<serde_json::Value>,
}

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
        silent,
        events_out_tx,
        state_manager,
        policy,
        memory,
        gatekeeper,
        wrapper_start_data,
    } = args;

    let mut state_handle: Option<StateManagerHandle> = None;
    let mut state_session_id: Option<String> = None;
    if let Some(manager) = state_manager.as_ref() {
        let handle = manager.handle();
        let session_id = handle
            .create_session(Some(run_id.clone()))
            .await
            .map_err(|e| RunnerError::Spawn(e.to_string()))?;
        handle
            .transition_phase(&session_id, RuntimePhase::Initializing)
            .await
            .map_err(|e| RunnerError::Spawn(e.to_string()))?;
        state_handle = Some(handle);
        state_session_id = Some(session_id);
    }

    let inject_cfg: InjectConfig = InjectConfig {
        placement: match cfg.prompt_inject.placement {
            crate::config::PromptInjectPlacement::System => InjectPlacement::System,
            crate::config::PromptInjectPlacement::User => InjectPlacement::User,
        },
        max_items: cfg.prompt_inject.max_items,
        max_answer_chars: cfg.prompt_inject.max_answer_chars,
        include_meta_line: cfg.prompt_inject.include_meta_line,
    };

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
        crate::config::MemoryProvider::Service(svc_cfg) => (svc_cfg.search_limit, svc_cfg.min_score),
    };

    let qa_ctx = EngineContext {
        project_id: &cfg.project_id,
        inject_cfg: &inject_cfg,
        cand_cfg: &cand_cfg,
        memory: memory.as_deref(),
        gatekeeper: gatekeeper.as_ref(),
        state_handle: state_handle.as_ref(),
        state_manager: state_manager.as_ref(),
        session_id: state_session_id.as_deref(),
        events_out: events_out_tx.as_ref(),
        memory_search_limit,
        memory_min_score,
    };

    let pre = pre_run(&qa_ctx, &user_query).await;
    let merged_query = pre.merged_query;
    let shown_qa_ids = pre.shown_qa_ids;
    let matches = pre.matches;
    let memory_search_event = pre.memory_search_event;

    // Buffer early wrapper events until we learn the effective run_id (session_id).
    // This keeps IDs consistent across the whole wrapper-event stream.
    let mut pending_wrapper_events: Vec<WrapperEvent> = Vec::new();
    if let Some(ev) = memory_search_event {
        pending_wrapper_events.push(ev);
    }

    let mut start_event = WrapperEvent::new("run.start", Utc::now().to_rfc3339());
    start_event.data = wrapper_start_data;
    pending_wrapper_events.push(start_event);

    // Build runner + session args (backend plan runs after memory injection)
    let (runner, session_args) = match runner {
        RunnerSpec::Backend {
            strategy,
            backend_spec,
            base_envs,
            resume_id,
            model,
            stream,
            stream_format,
        } => {
            let BackendPlan { runner, session_args } = strategy
                .plan(
                    &backend_spec,
                    base_envs,
                    resume_id,
                    merged_query.clone(),
                    model,
                    stream,
                    &stream_format,
                )
                .map_err(|e| RunnerError::Spawn(e.to_string()))?;
            (runner, session_args)
        }
        RunnerSpec::Passthrough { runner, session_args } => (runner, session_args),
    };

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

    if let (Some(handle), Some(session_id)) = (&state_handle, state_session_id.as_deref()) {
        handle
            .transition_phase(session_id, RuntimePhase::RunnerStarting)
            .await
            .map_err(|e| RunnerError::Spawn(e.to_string()))?;
    }

    // Start Session
    let session = match runner.start_session(&session_args).await {
        Ok(session) => session,
        Err(e) => {
            if let (Some(handle), Some(session_id)) = (&state_handle, state_session_id.as_deref()) {
                let _ = handle.fail(session_id, e.to_string()).await;
            }
            return Err(RunnerError::Spawn(e.to_string()));
        }
    };

    if let (Some(handle), Some(session_id)) = (&state_handle, state_session_id.as_deref()) {
        handle
            .transition_phase(session_id, RuntimePhase::RunnerRunning)
            .await
            .map_err(|e| RunnerError::Spawn(e.to_string()))?;
    }

    let run_input = RunSessionInput {
        session,
        run_id: run_id.clone(),
        control: cfg.control.clone(),
        policy,
        capture_bytes,
        events_out_tx: events_out_tx.clone(),
        silent,
        state_manager: state_manager.clone(),
        state_session_id: state_session_id.clone(),
    };

    // Run Session (runner runtime is in core; caller may provide a custom session loop, e.g. TUI).
    let run_result = match run_session_fn(run_input).await {
        Ok(r) => r,
        Err(e) => {
            if let (Some(handle), Some(session_id)) = (&state_handle, state_session_id.as_deref()) {
                let _ = handle.fail(session_id, e.to_string()).await;
            }
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
        let mut ev = WrapperEvent::new("tee.drop", Utc::now().to_rfc3339());
        ev.run_id = Some(effective_run_id.clone());
        ev.data = Some(serde_json::json!({ "dropped_lines": run_result.dropped_lines }));
        write_wrapper_event(events_out_tx.as_ref(), &ev).await;
    }

    let (run_outcome, _decision) =
        post_run(&qa_ctx, &run_result, &matches, shown_qa_ids, &user_query).await?;

    if let (Some(handle), Some(session_id)) = (&state_handle, state_session_id.as_deref()) {
        handle
            .complete(session_id, run_outcome.exit_code)
            .await
            .map_err(|e| RunnerError::Spawn(e.to_string()))?;
    }

    let mut exit_event = WrapperEvent::new("run.end", Utc::now().to_rfc3339());
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

    Ok(run_outcome.exit_code)
}

struct EngineContext<'a> {
    project_id: &'a str,
    inject_cfg: &'a InjectConfig,
    cand_cfg: &'a CandidateExtractConfig,
    memory: Option<&'a dyn MemoryPlugin>,
    gatekeeper: &'a dyn GatekeeperPlugin,
    state_handle: Option<&'a StateManagerHandle>,
    state_manager: Option<&'a Arc<StateManager>>,
    session_id: Option<&'a str>,
    events_out: Option<&'a EventsOutTx>,
    memory_search_limit: u32,
    memory_min_score: f32,
}

struct PreRun {
    merged_query: String,
    shown_qa_ids: Vec<String>,
    matches: Vec<SearchMatch>,
    memory_search_event: Option<WrapperEvent>,
}

async fn pre_run(ctx: &EngineContext<'_>, user_query: &str) -> PreRun {
    if let (Some(handle), Some(session_id)) = (ctx.state_handle, ctx.session_id) {
        let _ = handle.transition_phase(session_id, RuntimePhase::MemorySearch).await;
    }

    let Some(mem) = ctx.memory else {
        return PreRun {
            merged_query: user_query.to_string(),
            shown_qa_ids: vec![],
            matches: vec![],
            memory_search_event: None,
        };
    };

    let payload = QASearchPayload {
        project_id: ctx.project_id.to_string(),
        query: user_query.to_string(),
        limit: ctx.memory_search_limit,
        min_score: ctx.memory_min_score,
    };

    let matches = match mem.search(payload).await {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("memory search failed: {}", e);
            return PreRun {
                merged_query: user_query.to_string(),
                shown_qa_ids: vec![],
                matches: vec![],
                memory_search_event: None,
            };
        }
    };

    if let (Some(manager), Some(session_id)) = (ctx.state_manager, ctx.session_id) {
        if let Err(e) = manager
            .update_session(session_id, |session| {
                session.increment_memory_hits(matches.len());
            })
            .await
        {
            tracing::warn!("state update failed (memory hits): {}", e);
        }
        manager.emit_memory_hit(session_id, matches.len()).await;
    }

    let mut ev = WrapperEvent::new("memory.search.result", chrono::Utc::now().to_rfc3339());
    ev.data = Some(serde_json::json!({
        "query": user_query,
        "matches": matches.clone(),
    }));

    let run_outcome = RunOutcome {
        exit_code: 0,
        duration_ms: None,
        stdout_tail: String::new(),
        stderr_tail: String::new(),
        tool_events: vec![],
        shown_qa_ids: vec![],
        used_qa_ids: vec![],
    };

    let decision = ctx
        .gatekeeper
        .evaluate(chrono::Utc::now(), &matches, &run_outcome, &run_outcome.tool_events);

    let memory_ctx = render_memory_context(&decision.inject_list, ctx.inject_cfg);
    let merged = merge_prompt(user_query, &memory_ctx);
    let shown = decision
        .inject_list
        .iter()
        .map(|x| x.qa_id.clone())
        .collect();

    PreRun {
        merged_query: merged,
        shown_qa_ids: shown,
        matches,
        memory_search_event: Some(ev),
    }
}

async fn post_run(
    ctx: &EngineContext<'_>,
    run: &RunnerResult,
    matches: &[SearchMatch],
    shown_qa_ids: Vec<String>,
    user_query: &str,
) -> Result<(RunOutcome, GatekeeperDecision), RunnerError> {
    if let (Some(handle), Some(session_id)) = (ctx.state_handle, ctx.session_id) {
        handle
            .transition_phase(session_id, RuntimePhase::GatekeeperEvaluating)
            .await
            .map_err(|e| RunnerError::Spawn(e.to_string()))?;
    }

    let run_outcome = RunOutcome {
        exit_code: run.exit_code,
        duration_ms: run.duration_ms,
        stdout_tail: run.stdout_tail.clone(),
        stderr_tail: run.stderr_tail.clone(),
        tool_events: run.tool_events.clone(),
        shown_qa_ids,
        used_qa_ids: crate::gatekeeper::extract_qa_refs(&run.stdout_tail),
    };

    let decision = ctx
        .gatekeeper
        .evaluate(chrono::Utc::now(), matches, &run_outcome, &run.tool_events);

    if let (Some(manager), Some(session_id)) = (ctx.state_manager, ctx.session_id) {
        let signals = decision
            .signals
            .as_object()
            .map(|map| map.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();
        manager
            .update_session(session_id, |session| {
                session.set_gatekeeper_decision(GatekeeperDecisionSnapshot {
                    should_write_candidate: decision.should_write_candidate,
                    reasons: decision.reasons.clone(),
                    signals,
                });
            })
            .await
            .map_err(|e| RunnerError::Spawn(e.to_string()))?;
        manager
            .emit_gatekeeper_decision(session_id, decision.should_write_candidate)
            .await;
    }

    let mut decision_event =
        WrapperEvent::new("gatekeeper.decision", chrono::Utc::now().to_rfc3339());
    decision_event.run_id = Some(run.run_id.clone());
    decision_event.data = Some(serde_json::json!({
        "decision": serde_json::to_value(&decision).unwrap_or(serde_json::Value::Null),
    }));
    write_wrapper_event(ctx.events_out, &decision_event).await;

    if let (Some(mem), Some(handle), Some(session_id)) = (ctx.memory, ctx.state_handle, ctx.session_id)
    {
        handle
            .transition_phase(session_id, RuntimePhase::MemoryPersisting)
            .await
            .map_err(|e| RunnerError::Spawn(e.to_string()))?;

        let tool_events_lite: Vec<ToolEventLite> = run.tool_events.iter().map(|e| e.into()).collect();

        let candidate_drafts: Vec<CandidateDraft> = if decision.should_write_candidate {
            crate::memory::extract_candidates(
                ctx.cand_cfg,
                user_query,
                &run_outcome.stdout_tail,
                &run_outcome.stderr_tail,
                &tool_events_lite,
            )
        } else {
            vec![]
        };

        if let Some(hit_payload) = build_hit_payload(ctx.project_id, &decision) {
            let _ = mem.record_hit(hit_payload).await;
        }
        for v in build_validate_payloads(ctx.project_id, &decision) {
            let _ = mem.record_validation(v).await;
        }
        if decision.should_write_candidate && !candidate_drafts.is_empty() {
            let payloads = build_candidate_payloads(ctx.project_id, &candidate_drafts);
            for c in payloads {
                let _ = mem.record_candidate(c).await;
            }
        }
    }

    Ok((run_outcome, decision))
}
