use chrono::Utc;

use crate::commands::cli::{Args, BackendKind, RunArgs, TaskLevel};
use memex_core::config::MemoryProvider;
use memex_core::context::AppContext;
use memex_core::error::RunnerError;
use memex_core::events_out::write_wrapper_event;
use memex_core::gatekeeper::config::GatekeeperConfig as LogicGatekeeperConfig;
use memex_core::gatekeeper::{Gatekeeper, GatekeeperDecision, SearchMatch};
use memex_core::memory::InjectPlacement;
use memex_core::memory::MemoryPlugin;
use memex_core::memory::{
    build_candidate_payloads, build_hit_payload, build_validate_payloads, extract_candidates,
    merge_prompt, render_memory_context, CandidateDraft, CandidateExtractConfig, QASearchPayload,
};
use memex_core::runner::{run_session, RunOutcome, RunnerResult, RunnerStartArgs};
use memex_core::state::types::{GatekeeperDecisionSnapshot, RuntimePhase, StateEvent};
use memex_core::state::StateManagerHandle;
use memex_core::tool_event::{ToolEventLite, WrapperEvent};
use tokio::sync::mpsc;

use memex_plugins::factory;

#[tracing::instrument(name = "cli.run_app", skip(args, run_args, ctx))]
pub async fn run_app_with_config(
    args: Args,
    run_args: Option<RunArgs>,
    recover_run_id: Option<String>,
    ctx: &AppContext,
) -> Result<i32, RunnerError> {
    let args = args;
    let mut cfg = ctx.cfg().clone();
    let state_manager = ctx.state_manager();

    let mut prompt_text: Option<String> = None;

    if let Some(ra) = &run_args {
        if let Some(prompt) = &ra.prompt {
            prompt_text = Some(prompt.clone());
        } else if let Some(path) = &ra.prompt_file {
            let content = std::fs::read_to_string(path)
                .map_err(|e| RunnerError::Spawn(format!("failed to read prompt file: {}", e)))?;
            prompt_text = Some(content);
        } else if ra.stdin {
            use std::io::Read;
            let mut content = String::new();
            std::io::stdin().read_to_string(&mut content).map_err(|e| {
                RunnerError::Spawn(format!("failed to read prompt from stdin: {}", e))
            })?;
            prompt_text = Some(content);
        }

        if let Some(pid) = &ra.project_id {
            cfg.project_id = pid.clone();
        }

        if let Some(url) = &ra.memory_base_url {
            let MemoryProvider::Service(ref mut svc_cfg) = cfg.memory.provider;
            svc_cfg.base_url = url.clone();
        }
        if let Some(key) = &ra.memory_api_key {
            let MemoryProvider::Service(ref mut svc_cfg) = cfg.memory.provider;
            svc_cfg.api_key = key.clone();
        }
    }

    let mut stream_format = run_args
        .as_ref()
        .map(|ra| ra.stream_format.clone())
        .unwrap_or_else(|| "text".to_string());
    let mut stream_enabled = run_args.as_ref().map(|ra| ra.stream).unwrap_or(false);
    let force_tui = run_args.as_ref().map(|ra| ra.tui).unwrap_or(false);

    if force_tui {
        stream_enabled = true;
        if stream_format != "text" {
            stream_format = "text".to_string();
        }
    }

    let stream = factory::build_stream(&stream_format);
    let stream_plan = stream.apply(&mut cfg);
    let mut should_use_tui = force_tui;
    tracing::info!("TUI force enabled: {}", force_tui);
    tracing::info!("TUI config enabled: {}", cfg.tui.enabled);
    if !cfg.tui.enabled {
        should_use_tui = false;
    }
    if should_use_tui {
        if let Err(reason) = crate::tui::check_tui_support() {
            tracing::debug!("TUI disabled: {}", reason);
            should_use_tui = false;
        }
    }

    let events_out_tx = ctx.events_out();

    let memory = factory::build_memory(&cfg).map_err(|e| RunnerError::Spawn(e.to_string()))?;
    let policy = factory::build_policy(&cfg);
    let gatekeeper = factory::build_gatekeeper(&cfg);

    let run_id = recover_run_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    tracing::debug!(run_id = %run_id, stream_format = %stream_format, "run initialized");

    let mut tui_terminal = if should_use_tui {
        Some(crate::tui::setup_terminal().map_err(RunnerError::Spawn)?)
    } else {
        None
    };
    let mut tui_app = if should_use_tui {
        Some(crate::tui::TuiApp::new(cfg.tui.clone(), run_id.clone()))
    } else {
        None
    };

    // if should_use_tui && prompt_text.is_none() {
    //     let term = tui_terminal.as_mut().unwrap();
    //     let app = tui_app.as_mut().unwrap();
    //     match crate::tui::prompt_in_tui(term, app).await {
    //         Ok(text) => {
    //             prompt_text = Some(text);
    //             app.input_buffer.clear();
    //             app.input_cursor = 0;
    //             app.pending_qa = true;
    //             app.qa_started_at = Some(std::time::Instant::now());
    //         }
    //         Err(err) => {
    //             crate::tui::restore_terminal(term);
    //             return Err(err);
    //         }
    //     }
    // }

    let user_query = prompt_text
        .clone()
        .unwrap_or_else(|| args.codecli_args.join(" "));

    let effective_task_level = match run_args.as_ref().map(|ra| ra.task_level) {
        Some(TaskLevel::Auto) | None => infer_task_level(&user_query),
        Some(lv) => lv,
    };
    tracing::info!(task_level = ?effective_task_level, "task level selected");

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
        MemoryProvider::Service(svc_cfg) => (svc_cfg.search_limit, svc_cfg.min_score),
    };

    if let (Some(handle), Some(session_id)) = (&state_handle, state_session_id.as_deref()) {
        handle
            .transition_phase(session_id, RuntimePhase::MemorySearch)
            .await
            .map_err(|e| RunnerError::Spawn(e.to_string()))?;
    }

    let (_merged_query, shown_qa_ids, matches, memory_search_event) = build_merged_prompt(
        memory.as_deref(),
        &cfg.project_id,
        &user_query,
        memory_search_limit,
        memory_min_score,
        &gk_logic_cfg,
        &inject_cfg,
    )
    .await;

    if should_use_tui {
        if let Some(app) = tui_app.as_mut() {
            app.pending_qa = false;
        }
    }

    if let (Some(manager), Some(session_id)) = (&state_manager, state_session_id.as_deref()) {
        manager
            .update_session(session_id, |session| {
                session.increment_memory_hits(matches.len());
            })
            .await
            .map_err(|e| RunnerError::Spawn(e.to_string()))?;
        manager.emit_memory_hit(session_id, matches.len()).await;
    }

    // Buffer early wrapper events until we learn the effective run_id (session_id).
    // This keeps IDs consistent across the whole wrapper-event stream.
    let mut pending_wrapper_events: Vec<WrapperEvent> = Vec::new();
    if let Some(ev) = memory_search_event {
        pending_wrapper_events.push(ev);
    }

    let mut start_event = WrapperEvent::new("run.start", Utc::now().to_rfc3339());

    // Build backend plan (runner + session args)
    let mut base_envs: std::collections::HashMap<String, String> = std::env::vars().collect();

    let (runner, session_args) = if let Some(ra) = &run_args {
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
                user_query.clone(),
                ra.model.clone(),
                stream_enabled,
                &stream_format,
            )
            .map_err(|e| RunnerError::Spawn(e.to_string()))?;

        (backend_plan.runner, backend_plan.session_args)
    } else {
        // Legacy mode (no subcommand): passthrough cmd/args exactly as provided.
        let runner = factory::build_runner(&cfg);
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

    let silent = stream_plan.silent || should_use_tui;

    // Run Session (Core Logic)
    let run_result = match if should_use_tui {
        let (tui_tx, tui_rx) = mpsc::unbounded_channel();
        let control = cfg.control.clone();
        let run_id_clone = run_id.clone();
        let state_manager_run = state_manager.clone();
        let state_session_id_run = state_session_id.clone();
        let state_session_id_tui = state_session_id.clone();
        let tui_tx_state = tui_tx.clone();
        if let Some(manager) = state_manager.as_ref() {
            let mut state_rx = manager.subscribe();
            tokio::spawn(async move {
                let mut phase = RuntimePhase::Initializing;
                let mut memory_hits = 0usize;
                let mut tool_events = 0usize;
                loop {
                    match state_rx.recv().await {
                        Ok(event) => {
                            let Some(session_id) = event.session_id() else {
                                continue;
                            };
                            if state_session_id_tui.as_deref() != Some(session_id) {
                                continue;
                            }
                            match event {
                                StateEvent::SessionStateChanged { new_phase, .. } => {
                                    phase = new_phase;
                                }
                                StateEvent::ToolEventReceived { event_count, .. } => {
                                    tool_events = tool_events.saturating_add(event_count);
                                }
                                StateEvent::MemoryHit { hit_count, .. } => {
                                    memory_hits = hit_count;
                                }
                                _ => {}
                            }
                            let _ = tui_tx_state.send(memex_core::tui::TuiEvent::StateUpdate {
                                phase,
                                memory_hits,
                                tool_events,
                            });
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    }
                }
            });
        }
        let events_out_tx_run = events_out_tx.clone();
        let run_task = tokio::spawn(async move {
            run_session(
                session,
                &control,
                policy,
                args.capture_bytes,
                events_out_tx_run,
                Some(tui_tx),
                &run_id_clone,
                silent,
                state_manager_run,
                state_session_id_run,
            )
            .await
        });
        let term = tui_terminal
            .as_mut()
            .ok_or_else(|| RunnerError::Spawn("TUI terminal not initialized".to_string()))?;
        let app = tui_app
            .as_mut()
            .ok_or_else(|| RunnerError::Spawn("TUI app not initialized".to_string()))?;
        let result = crate::tui::run_with_tui_on_terminal(term, app, tui_rx, run_task).await;
        crate::tui::restore_terminal(term);
        result
    } else {
        run_session(
            session,
            &cfg.control,
            policy,
            args.capture_bytes,
            events_out_tx.clone(),
            None,
            &run_id,
            silent,
            state_manager.clone(),
            state_session_id.clone(),
        )
        .await
    } {
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

    let run_outcome: RunOutcome = build_run_outcome(&run_result, shown_qa_ids);

    if let (Some(handle), Some(session_id)) = (&state_handle, state_session_id.as_deref()) {
        handle
            .transition_phase(session_id, RuntimePhase::GatekeeperEvaluating)
            .await
            .map_err(|e| RunnerError::Spawn(e.to_string()))?;
    }

    let decision = gatekeeper.evaluate(Utc::now(), &matches, &run_outcome, &run_result.tool_events);

    if let (Some(manager), Some(session_id)) = (&state_manager, state_session_id.as_deref()) {
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

    let mut decision_event = WrapperEvent::new("gatekeeper.decision", Utc::now().to_rfc3339());
    decision_event.run_id = Some(effective_run_id.clone());
    decision_event.data = Some(serde_json::json!({
        "decision": serde_json::to_value(&decision).unwrap_or(serde_json::Value::Null),
    }));
    write_wrapper_event(events_out_tx.as_ref(), &decision_event).await;

    if let Some(mem) = &memory {
        if let (Some(handle), Some(session_id)) = (&state_handle, state_session_id.as_deref()) {
            handle
                .transition_phase(session_id, RuntimePhase::MemoryPersisting)
                .await
                .map_err(|e| RunnerError::Spawn(e.to_string()))?;
        }

        let tool_events_lite: Vec<ToolEventLite> =
            run_result.tool_events.iter().map(|e| e.into()).collect();

        let candidate_drafts = if decision.should_write_candidate {
            extract_candidates(
                &cand_cfg,
                &user_query,
                &run_outcome.stdout_tail,
                &run_outcome.stderr_tail,
                &tool_events_lite,
            )
        } else {
            vec![]
        };

        post_run_memory_reporting(mem.as_ref(), &cfg.project_id, &decision, candidate_drafts).await;
    }

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

fn infer_task_level(prompt: &str) -> TaskLevel {
    let s = prompt.trim();
    if s.is_empty() {
        return TaskLevel::L1;
    }

    let lower = s.to_ascii_lowercase();

    // Strong engineering / multi-step signals => L2
    if lower.contains("architecture")
        || lower.contains("系统架构")
        || lower.contains("设计")
        || lower.contains("debug")
        || lower.contains("根因")
        || lower.contains("refactor")
        || lower.contains("重构")
        || lower.contains("compile")
        || lower.contains("cargo")
        || lower.contains("stack trace")
        || lower.contains("日志")
        || lower.contains("测试")
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
        || lower.contains("文案")
        || lower.contains("世界观")
        || lower.contains("小说")
        || lower.contains("分镜")
    {
        return TaskLevel::L3;
    }

    // Very short tool-like requests => L0
    if s.chars().count() <= 200
        && (lower.contains("translate")
            || lower.contains("翻译")
            || lower.contains("format")
            || lower.contains("格式化")
            || lower.contains("json")
            || lower.contains("rewrite")
            || lower.contains("改写"))
    {
        return TaskLevel::L0;
    }

    TaskLevel::L1
}

fn build_run_outcome(run: &RunnerResult, shown_qa_ids: Vec<String>) -> RunOutcome {
    RunOutcome {
        exit_code: run.exit_code,
        duration_ms: run.duration_ms,
        stdout_tail: run.stdout_tail.clone(),
        stderr_tail: run.stderr_tail.clone(),
        tool_events: run.tool_events.clone(),
        shown_qa_ids,
        used_qa_ids: memex_core::gatekeeper::extract_qa_refs(&run.stdout_tail),
    }
}

async fn post_run_memory_reporting(
    mem: &dyn MemoryPlugin,
    project_id: &str,
    decision: &GatekeeperDecision,
    candidate_drafts: Vec<CandidateDraft>,
) {
    if let Some(hit_payload) = build_hit_payload(project_id, decision) {
        let _ = mem.record_hit(hit_payload).await;
    }

    for v in build_validate_payloads(project_id, decision) {
        let _ = mem.record_validation(v).await;
    }

    if decision.should_write_candidate && !candidate_drafts.is_empty() {
        let payloads = build_candidate_payloads(project_id, &candidate_drafts);
        for c in payloads {
            let _ = mem.record_candidate(c).await;
        }
    }
}

async fn build_merged_prompt(
    memory: Option<&dyn MemoryPlugin>,
    project_id: &str,
    user_query: &str,
    limit: u32,
    min_score: f32,
    gk_cfg: &LogicGatekeeperConfig,
    inject_cfg: &memex_core::memory::InjectConfig,
) -> (String, Vec<String>, Vec<SearchMatch>, Option<WrapperEvent>) {
    if memory.is_none() {
        return (user_query.to_string(), vec![], vec![], None);
    }
    let mem = memory.unwrap();

    let payload = QASearchPayload {
        project_id: project_id.to_string(),
        query: user_query.to_string(),
        limit,
        min_score,
    };

    let matches = match mem.search(payload).await {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("memory search failed: {}", e);
            return (user_query.to_string(), vec![], vec![], None);
        }
    };

    let mut ev = WrapperEvent::new("memory.search.result", Utc::now().to_rfc3339());
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
        gk_cfg,
        Utc::now(),
        &matches,
        &run_outcome,
        &run_outcome.tool_events,
    );

    let memory_ctx = render_memory_context(&decision.inject_list, inject_cfg);
    let merged = merge_prompt(user_query, &memory_ctx);
    let shown = decision
        .inject_list
        .iter()
        .map(|x| x.qa_id.clone())
        .collect();

    (merged, shown, matches, Some(ev))
}

fn parse_env_file(path: &str) -> Result<Vec<(String, String)>, RunnerError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| RunnerError::Spawn(format!("failed to read env file: {}", e)))?;
    let mut out = Vec::new();

    for (idx, raw_line) in content.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() {
            return Err(RunnerError::Spawn(format!(
                "env file contains empty line at {}",
                idx + 1
            )));
        }
        if line.starts_with('#') {
            continue;
        }
        let (k, v) = line.split_once('=').ok_or_else(|| {
            RunnerError::Spawn(format!(
                "invalid env line at {} (expected KEY=VALUE)",
                idx + 1
            ))
        })?;
        let key = k.trim();
        if key.is_empty() {
            return Err(RunnerError::Spawn(format!(
                "invalid env line at {} (empty key)",
                idx + 1
            )));
        }
        let value = parse_env_value(v.trim(), idx + 1)?;
        out.push((key.to_string(), value));
    }

    Ok(out)
}

fn parse_env_value(value: &str, line_no: usize) -> Result<String, RunnerError> {
    if value.len() >= 2 {
        let first = value.chars().next().unwrap();
        let last = value.chars().last().unwrap();
        if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
            let inner = &value[1..value.len() - 1];
            return unescape_env_value(inner, line_no);
        }
    }
    Ok(value.to_string())
}

fn unescape_env_value(value: &str, line_no: usize) -> Result<String, RunnerError> {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        let Some(next) = chars.next() else {
            return Err(RunnerError::Spawn(format!(
                "invalid escape at line {} (trailing backslash)",
                line_no
            )));
        };
        match next {
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            '\\' => out.push('\\'),
            '"' => out.push('"'),
            '\'' => out.push('\''),
            other => out.push(other),
        }
    }
    Ok(out)
}
