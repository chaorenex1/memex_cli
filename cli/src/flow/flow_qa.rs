use chrono::Utc;
use memex_core::config::AppConfig;
use memex_core::error::RunnerError;
use memex_core::events_out::{write_wrapper_event, EventsOutTx};
use memex_core::gatekeeper::config::GatekeeperConfig as LogicGatekeeperConfig;
use memex_core::gatekeeper::{Gatekeeper, GatekeeperDecision, GatekeeperPlugin, SearchMatch};
use memex_core::memory::CandidateExtractConfig;
use memex_core::memory::InjectPlacement;
use memex_core::memory::MemoryPlugin;
use memex_core::memory::{
    build_candidate_payloads, build_hit_payload, build_validate_payloads, merge_prompt,
    render_memory_context, CandidateDraft, QASearchPayload,
};
use memex_core::runner::{PolicyPlugin, RunOutcome, RunnerResult, RunnerSession, RunnerStartArgs};
use memex_core::state::types::{GatekeeperDecisionSnapshot, RuntimePhase};
use memex_core::state::{StateManager, StateManagerHandle};
use memex_core::tool_event::{ToolEventLite, WrapperEvent};
use memex_plugins::factory;
use std::sync::Arc;

use crate::commands::cli::{Args, BackendKind, RunArgs, TaskLevel};
use crate::flow::tui;
use crate::utils::parse_env_file;
use std::future::Future;

pub struct RunSessionInput {
    pub session: Box<dyn RunnerSession>,
    pub run_id: String,
    pub control: memex_core::config::ControlConfig,
    pub policy: Option<Box<dyn PolicyPlugin>>,
    pub capture_bytes: usize,
    pub events_out_tx: Option<EventsOutTx>,
    pub silent: bool,
    pub state_manager: Option<Arc<StateManager>>,
    pub state_session_id: Option<String>,
}

pub async fn run_with_query<F, Fut>(
    user_query: String,
    args: &Args,
    run_args: Option<&RunArgs>,
    cfg: &mut AppConfig,
    state_manager: Option<Arc<StateManager>>,
    events_out_tx: Option<EventsOutTx>,
    run_id: String,
    recover_run_id: Option<String>,
    should_use_tui: bool,
    stream_enabled: bool,
    stream_format: &str,
    stream_silent: bool,
    policy: Option<Box<dyn PolicyPlugin>>,
    memory: Option<Box<dyn MemoryPlugin>>,
    gatekeeper: Box<dyn memex_core::gatekeeper::GatekeeperPlugin>,
    tui_runtime: Option<*mut tui::TuiRuntime>,
    run_session_fn: F,
) -> Result<i32, RunnerError>
where
    F: FnOnce(RunSessionInput) -> Fut,
    Fut: Future<Output = Result<RunnerResult, RunnerError>>,
{
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

    let gk_logic_cfg: LogicGatekeeperConfig = cfg.gatekeeper_logic_config();

    let inject_cfg = memex_core::memory::InjectConfig {
        placement: match cfg.prompt_inject.placement {
            memex_core::config::PromptInjectPlacement::System => InjectPlacement::System,
            memex_core::config::PromptInjectPlacement::User => InjectPlacement::User,
        },
        max_items: cfg.prompt_inject.max_items,
        max_answer_chars: cfg.prompt_inject.max_answer_chars,
        include_meta_line: cfg.prompt_inject.include_meta_line,
    };

    let cand_cfg = CandidateExtractConfig {
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
        memex_core::config::MemoryProvider::Service(svc_cfg) => {
            (svc_cfg.search_limit, svc_cfg.min_score)
        }
    };

    let qa_ctx = QaContext {
        project_id: &cfg.project_id,
        inject_cfg: &inject_cfg,
        cand_cfg: &cand_cfg,
        gk_cfg: &gk_logic_cfg,
        memory: memory.as_deref(),
        gatekeeper: gatekeeper.as_ref(),
        state_handle: state_handle.as_ref(),
        state_manager: state_manager.as_ref(),
        session_id: state_session_id.as_deref(),
        events_out: events_out_tx.as_ref(),
        memory_search_limit,
        memory_min_score,
    };

    let pre = qa_pre_run(&qa_ctx, &user_query).await;
    let merged_query = pre.merged_query;
    let shown_qa_ids = pre.shown_qa_ids;
    let matches = pre.matches;
    let memory_search_event = pre.memory_search_event;

    if should_use_tui {
        if let Some(ptr) = tui_runtime {
            unsafe {
                (*ptr).app.pending_qa = false;
            }
        }
    }

    // Buffer early wrapper events until we learn the effective run_id (session_id).
    // This keeps IDs consistent across the whole wrapper-event stream.
    let mut pending_wrapper_events: Vec<WrapperEvent> = Vec::new();
    if let Some(ev) = memory_search_event {
        pending_wrapper_events.push(ev);
    }

    let mut start_event = WrapperEvent::new("run.start", Utc::now().to_rfc3339());
    let effective_task_level = match run_args.map(|ra| ra.task_level) {
        Some(TaskLevel::Auto) | None => infer_task_level(&user_query),
        Some(lv) => lv,
    };
    tracing::info!(task_level = ?effective_task_level, "task level selected");

    // Build backend plan (runner + session args)
    let mut base_envs: std::collections::HashMap<String, String> = std::env::vars().collect();

    let (runner, session_args) = if let Some(ra) = run_args {
        let backend_spec = ra.backend.as_str();
        let backend_kind = ra.backend_kind.map(|kind| match kind {
            BackendKind::Codecli => "codecli",
            BackendKind::Aiservice => "aiservice",
        });
        if let Some(kind) = backend_kind {
            cfg.backend_kind = kind.to_string();
        }
        let backend = match backend_kind {
            Some(kind) => factory::build_backend_with_kind(kind, backend_spec),
            None => factory::build_backend(backend_spec),
        };
        if let Some(path) = &ra.env_file {
            let file_envs = parse_env_file(path)?;
            for (k, v) in file_envs {
                base_envs.insert(k, v);
            }
        }

        // Merge extra envs from CLI flags (KEY=VALUE), overriding process env.
        for kv in ra.env.iter() {
            if let Some((k, v)) = kv.split_once('=') {
                if !k.trim().is_empty() {
                    base_envs.insert(k.trim().to_string(), v.to_string());
                }
            }
        }

        let backend_plan = backend
            .plan(
                backend_spec,
                base_envs,
                recover_run_id.clone(),
                merged_query.clone(),
                ra.model.clone(),
                stream_enabled,
                stream_format,
            )
            .map_err(|e| RunnerError::Spawn(e.to_string()))?;

        (backend_plan.runner, backend_plan.session_args)
    } else {
        // Legacy mode (no subcommand): passthrough cmd/args exactly as provided.
        let runner = factory::build_runner(cfg);
        let session_args = RunnerStartArgs {
            cmd: args.codecli_bin.clone(),
            args: args.codecli_args.clone(),
            envs: base_envs,
        };
        (runner, session_args)
    };

    // Emit start event with the actual backend invocation
    start_event.data = Some(serde_json::json!({
        "cmd": session_args.cmd.clone(),
        "args": session_args.args.clone(),
        "task_level": format!("{effective_task_level:?}"),
    }));
    pending_wrapper_events.push(start_event);

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

    let silent = stream_silent || should_use_tui;

    let run_input = RunSessionInput {
        session,
        run_id: run_id.clone(),
        control: cfg.control.clone(),
        policy,
        capture_bytes: args.capture_bytes,
        events_out_tx: events_out_tx.clone(),
        silent,
        state_manager: state_manager.clone(),
        state_session_id: state_session_id.clone(),
    };

    // Run Session (Core Logic)
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
        qa_post_run(&qa_ctx, &run_result, &matches, shown_qa_ids, &user_query).await?;

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

pub struct QaContext<'a> {
    pub project_id: &'a str,
    pub inject_cfg: &'a memex_core::memory::InjectConfig,
    pub cand_cfg: &'a CandidateExtractConfig,
    pub gk_cfg: &'a LogicGatekeeperConfig,
    pub memory: Option<&'a dyn MemoryPlugin>,
    pub gatekeeper: &'a dyn GatekeeperPlugin,
    pub state_handle: Option<&'a StateManagerHandle>,
    pub state_manager: Option<&'a Arc<StateManager>>,
    pub session_id: Option<&'a str>,
    pub events_out: Option<&'a EventsOutTx>,
    pub memory_search_limit: u32,
    pub memory_min_score: f32,
}

pub struct QaPreRun {
    pub merged_query: String,
    pub shown_qa_ids: Vec<String>,
    pub matches: Vec<SearchMatch>,
    pub memory_search_event: Option<WrapperEvent>,
}

pub async fn qa_pre_run(ctx: &QaContext<'_>, user_query: &str) -> QaPreRun {
    if let (Some(handle), Some(session_id)) = (ctx.state_handle, ctx.session_id) {
        let _ = handle
            .transition_phase(session_id, RuntimePhase::MemorySearch)
            .await;
    }

    if ctx.memory.is_none() {
        return QaPreRun {
            merged_query: user_query.to_string(),
            shown_qa_ids: vec![],
            matches: vec![],
            memory_search_event: None,
        };
    }

    let mem = ctx.memory.unwrap();
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
            return QaPreRun {
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
        stdout_tail: "".to_string(),
        stderr_tail: "".to_string(),
        tool_events: vec![],
        shown_qa_ids: vec![],
        used_qa_ids: vec![],
    };

    let decision = Gatekeeper::evaluate(
        ctx.gk_cfg,
        chrono::Utc::now(),
        &matches,
        &run_outcome,
        &run_outcome.tool_events,
    );

    let memory_ctx = render_memory_context(&decision.inject_list, ctx.inject_cfg);
    let merged = merge_prompt(user_query, &memory_ctx);
    let shown = decision
        .inject_list
        .iter()
        .map(|x| x.qa_id.clone())
        .collect();

    QaPreRun {
        merged_query: merged,
        shown_qa_ids: shown,
        matches,
        memory_search_event: Some(ev),
    }
}

pub async fn qa_post_run(
    ctx: &QaContext<'_>,
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
        used_qa_ids: memex_core::gatekeeper::extract_qa_refs(&run.stdout_tail),
    };

    let decision =
        ctx.gatekeeper
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

    if let (Some(mem), Some(handle), Some(session_id)) =
        (ctx.memory, ctx.state_handle, ctx.session_id)
    {
        handle
            .transition_phase(session_id, RuntimePhase::MemoryPersisting)
            .await
            .map_err(|e| RunnerError::Spawn(e.to_string()))?;

        let tool_events_lite: Vec<ToolEventLite> =
            run.tool_events.iter().map(|e| e.into()).collect();

        let candidate_drafts: Vec<CandidateDraft> = if decision.should_write_candidate {
            memex_core::memory::extract_candidates(
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

fn infer_task_level(prompt: &str) -> TaskLevel {
    let s = prompt.trim();
    if s.is_empty() {
        return TaskLevel::L1;
    }

    let lower = s.to_ascii_lowercase();

    // Strong engineering / multi-step signals => L2
    if lower.contains("architecture")
        || lower.contains("绯荤粺鏋舵瀯")
        || lower.contains("璁捐")
        || lower.contains("debug")
        || lower.contains("鏍瑰洜")
        || lower.contains("refactor")
        || lower.contains("閲嶆瀯")
        || lower.contains("compile")
        || lower.contains("cargo")
        || lower.contains("stack trace")
        || lower.contains("鏃ュ織")
        || lower.contains("娴嬭瘯")
        || lower.contains("benchmark")
        || s.contains("```")
    {
        return TaskLevel::L2;
    }

    // High creativity / style-heavy signals => L3
    if lower.contains("story")
        || lower.contains("novel")
        || lower.contains("brand")
        || lower.contains("marketing")
        || lower.contains("style")
        || lower.contains("鏂囨")
        || lower.contains("涓栫晫瑙?")
        || lower.contains("灏忚")
        || lower.contains("鍒嗛暅")
    {
        return TaskLevel::L3;
    }

    // Very short tool-like requests => L0
    if s.chars().count() <= 200
        && (lower.contains("translate")
            || lower.contains("缈昏瘧")
            || lower.contains("format")
            || lower.contains("鏍煎紡鍖?")
            || lower.contains("json")
            || lower.contains("rewrite")
            || lower.contains("鏀瑰啓"))
    {
        return TaskLevel::L0;
    }

    TaskLevel::L1
}
