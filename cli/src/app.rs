use chrono::Utc;

use crate::commands::cli::{Args, RunArgs};
use memex_core::config::{load_default, MemoryProvider};
use memex_core::memory::InjectPlacement;
use memex_core::gatekeeper::config::GatekeeperConfig as LogicGatekeeperConfig;
use memex_core::error::RunnerError;
use memex_core::events_out::{start_events_out, EventsOutTx, write_wrapper_event};
use memex_core::gatekeeper::{Gatekeeper, SearchMatch, GatekeeperDecision};
use memex_core::memory::{
    build_candidate_payloads, build_hit_payload, build_validate_payloads, extract_candidates,
    merge_prompt, render_memory_context, CandidateExtractConfig,
    QASearchPayload, CandidateDraft
};
use memex_core::memory::MemoryPlugin;
use memex_core::runner::{run_session, RunOutcome, RunnerResult, RunnerStartArgs};
use memex_core::tool_event::{ToolEventLite, WrapperEvent};

use memex_plugins::factory;

pub async fn run_app(args: Args, run_args: Option<RunArgs>, recover_run_id: Option<String>) -> Result<i32, RunnerError> {
    let mut args = args;

    let mut cfg = load_default().map_err(|e| RunnerError::Spawn(e.to_string()))?;

    if let Some(ra) = &run_args {
        args.codecli_bin = ra.backend.clone();
        args.codecli_args = Vec::new();
        
        if let Some(model) = &ra.model {
            args.codecli_args.push("--model".to_string());
            args.codecli_args.push(model.clone());
        }

        if ra.stream {
            args.codecli_args.push("--stream".to_string());
        }

        if let Some(prompt) = &ra.prompt {
            args.codecli_args.push(prompt.clone());
        } else if let Some(path) = &ra.prompt_file {
            let content = std::fs::read_to_string(path)
                .map_err(|e| RunnerError::Spawn(format!("failed to read prompt file: {}", e)))?;
            args.codecli_args.push(content);
        } else if ra.stdin {
            use std::io::Read;
            let mut content = String::new();
            std::io::stdin().read_to_string(&mut content)
                .map_err(|e| RunnerError::Spawn(format!("failed to read prompt from stdin: {}", e)))?;
            args.codecli_args.push(content);
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

    let stream_format = run_args.as_ref().map(|ra| ra.stream_format.as_str()).unwrap_or("text");

    if stream_format == "jsonl" {
        cfg.events_out.enabled = true;
        cfg.events_out.path = "stdout:".to_string();
    }

    let user_query = args.codecli_args.join(" ");

    let events_out_tx = start_events_out(&cfg.events_out)
        .await
        .map_err(RunnerError::Spawn)?;

    let memory = factory::build_memory(&cfg).map_err(|e| RunnerError::Spawn(e.to_string()))?;
    let runner = factory::build_runner(&cfg);
    let policy = factory::build_policy(&cfg);
    let gatekeeper = factory::build_gatekeeper(&cfg);

    let run_id = recover_run_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

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

    let (_merged_query, shown_qa_ids, matches) = build_merged_prompt(
        memory.as_deref(),
        &cfg.project_id,
        &user_query,
        memory_search_limit,
        memory_min_score,
        &gk_logic_cfg,
        &inject_cfg,
        events_out_tx.as_ref(),
        &run_id,
    )
    .await;

    let mut start_event = WrapperEvent::new("run.start", Utc::now().to_rfc3339());
    start_event.run_id = Some(run_id.clone());
    start_event.data = Some(serde_json::json!({
        "cmd": args.codecli_bin.clone(),
        "args": args.codecli_args.clone(),
    }));
    write_wrapper_event(events_out_tx.as_ref(), &start_event).await;

    // Start Session
    let session_args = RunnerStartArgs {
        cmd: args.codecli_bin.clone(),
        args: args.codecli_args.clone(),
        envs: std::env::vars().collect(),
    };
    
    let session = runner.start_session(&session_args).await.map_err(|e| RunnerError::Spawn(e.to_string()))?;

    // Run Session (Core Logic)
    let run_result = run_session(session, &cfg.control, policy, args.capture_bytes, events_out_tx.clone(), &run_id, stream_format).await?;

    if run_result.dropped_lines > 0 {
        let mut ev = WrapperEvent::new("tee.drop", Utc::now().to_rfc3339());
        ev.run_id = Some(run_id.clone());
        ev.data = Some(serde_json::json!({ "dropped_lines": run_result.dropped_lines }));
        write_wrapper_event(events_out_tx.as_ref(), &ev).await;
    }

    let run_outcome: RunOutcome = build_run_outcome(&run_result, shown_qa_ids);

    let decision = gatekeeper.evaluate(
        Utc::now(),
        &matches,
        &run_outcome,
        &run_result.tool_events,
    );

    let mut decision_event = WrapperEvent::new("gatekeeper.decision", Utc::now().to_rfc3339());
    decision_event.run_id = Some(run_id.clone());
    decision_event.data = Some(serde_json::json!({
        "decision": serde_json::to_value(&decision).unwrap_or(serde_json::Value::Null),
    }));
    write_wrapper_event(events_out_tx.as_ref(), &decision_event).await;

    if let Some(mem) = &memory {
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

    let mut exit_event = WrapperEvent::new("run.end", Utc::now().to_rfc3339());
    exit_event.run_id = Some(run_id);
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
    events_out: Option<&EventsOutTx>,
    run_id: &str,
) -> (String, Vec<String>, Vec<SearchMatch>) {
    if memory.is_none() {
        return (user_query.to_string(), vec![], vec![]);
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
            return (user_query.to_string(), vec![], vec![]);
        }
    };
    
    let mut ev = WrapperEvent::new("memory.search.result", Utc::now().to_rfc3339());
    ev.run_id = Some(run_id.to_string());
    ev.data = Some(serde_json::json!({
        "query": user_query,
        "matches": matches.clone(),
    }));
    write_wrapper_event(events_out, &ev).await;

    let run_outcome = RunOutcome {
        exit_code: 0,
        duration_ms: None,
        stdout_tail: "".to_string(),
        stderr_tail: "".to_string(),
        tool_events: vec![],
        shown_qa_ids: vec![],
        used_qa_ids: vec![],
    };

    let decision =
        Gatekeeper::evaluate(gk_cfg, Utc::now(), &matches, &run_outcome, &run_outcome.tool_events);

    let memory_ctx = render_memory_context(&decision.inject_list, &inject_cfg);
    let merged = merge_prompt(user_query, &memory_ctx);
    let shown = decision.inject_list.iter().map(|x| x.qa_id.clone()).collect();

    (merged, shown, matches)
}