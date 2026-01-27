//! 引擎 post-run：基于 runner 输出与 tool events 进行 gatekeeper 评估，并按需向 memory 写入 hit/validation/candidate。
use crate::error::RunnerError;
use crate::events_out::write_wrapper_event;
use crate::gatekeeper::{GatekeeperDecision, GatekeeperPlugin, SearchMatch};
use crate::memory::{
    build_candidate_payloads, build_hit_payload, build_validate_payloads, CandidateDraft,
    CandidateExtractConfig, MemoryPlugin,
};
use crate::runner::{RunOutcome, RunnerResult};
use crate::tool_event::{ToolEventLite, WrapperEvent};

pub(crate) struct PostRunContext<'a> {
    pub project_id: &'a str,
    pub cand_cfg: &'a CandidateExtractConfig,
    pub memory: Option<&'a dyn MemoryPlugin>,
    pub gatekeeper: &'a dyn GatekeeperPlugin,
    pub events_out: Option<&'a crate::events_out::EventsOutTx>,
}

pub async fn post_run(
    run: &RunnerResult,
    pre: &super::pre::PreRun,
    project_id: &String,
    cfg: &crate::config::AppConfig,
    services: &crate::context::Services,
    events_out_tx: &Option<crate::events_out::EventsOutTx>,
    user_query: &str,
) -> Result<(RunOutcome, GatekeeperDecision), RunnerError> {
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

    let ctx = PostRunContext {
        project_id,
        cand_cfg: &cand_cfg,
        memory: services.memory.as_deref(),
        gatekeeper: services.gatekeeper.as_ref(),
        events_out: events_out_tx.as_ref(),
    };
    let matches: Vec<SearchMatch> = pre.matches.clone();
    let shown_qa_ids: Vec<String> = pre.shown_qa_ids.clone();

    tracing::info!(
        target: "memex.qa",
        stage = "post.start",
        project_id = %ctx.project_id,
        run_id = %run.run_id,
        exit_code = run.exit_code,
        matches = matches.len(),
        shown = shown_qa_ids.len(),
        user_query_len = user_query.len(),
        memory_enabled = ctx.memory.is_some()
    );
    let run_outcome = RunOutcome {
        exit_code: run.exit_code,
        duration_ms: run.duration_ms,
        stdout_tail: run.stdout_tail.clone(),
        stderr_tail: run.stderr_tail.clone(),
        tool_events: run.tool_events.clone(),
        shown_qa_ids,
        used_qa_ids: crate::gatekeeper::extract_qa_refs(&run.stdout_tail),
    };

    tracing::info!(
        target: "memex.qa",
        stage = "post.used_refs",
        used = run_outcome.used_qa_ids.len()
    );

    let mut decision = ctx.gatekeeper.evaluate(
        chrono::Local::now(),
        &matches,
        &run_outcome,
        &run.tool_events,
    );

    let mut decision_event =
        WrapperEvent::new("gatekeeper.decision", chrono::Local::now().to_rfc3339());
    decision_event.run_id = Some(run.run_id.clone());
    decision_event.data = Some(serde_json::json!({
        "decision": serde_json::to_value(&decision).unwrap_or(serde_json::Value::Null),
    }));
    write_wrapper_event(ctx.events_out, &decision_event).await;

    if let Some(mem) = ctx.memory {
        tracing::debug!(
            target: "memex.qa",
            stage = "post.memory.write_plan",
            should_write_candidate = decision.should_write_candidate,
            hit_refs = decision.hit_refs.len(),
            validate_plans = decision.validate_plans.len()
        );

        let tool_events_lite: Vec<ToolEventLite> =
            run.tool_events.iter().map(|e| e.into()).collect();

        let candidate_drafts: Vec<CandidateDraft> = if decision.should_write_candidate {
            tracing::debug!(target: "memex.qa", stage = "candidate.extract.in");
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

        let candidate_drafts_len = candidate_drafts.len();
        decision.candidate_drafts = candidate_drafts;
        tracing::debug!(
            target: "memex.qa",
            stage = "candidate.extract.out",
            drafts = candidate_drafts_len
        );

        // Parallel memory writes for better performance
        // Hit, validation, and candidate writes are independent operations
        let hit_future = async {
            if let Some(hit_payload) = build_hit_payload(ctx.project_id, &decision) {
                // Single-pass counting for used and shown references
                let (used, shown) = hit_payload.references.iter().fold((0, 0), |(u, s), r| {
                    (
                        u + usize::from(r.used == Some(true)),
                        s + usize::from(r.shown == Some(true)),
                    )
                });
                tracing::info!(
                    target: "memex.qa",
                    stage = "memory.hit.in",
                    references = hit_payload.references.len(),
                    shown = shown,
                    used = used
                );
                let result = mem.record_hit(hit_payload).await;
                if let Err(e) = &result {
                    tracing::warn!(
                        target: "memex.qa",
                        stage = "memory.hit.error",
                        error = %e,
                        "Failed to record memory hit (non-fatal)"
                    );
                }
                tracing::debug!(target: "memex.qa", stage = "memory.hit.out");
                result
            } else {
                Ok(())
            }
        };

        let validations_future = async {
            let validations = build_validate_payloads(ctx.project_id, &decision);
            let mut results = Vec::new();
            for v in validations {
                let qa_id = v.qa_id.clone();
                tracing::info!(
                    target: "memex.qa",
                    stage = "memory.validate.in",
                    qa_id = %qa_id,
                    result = ?v.result
                );
                let result = mem.record_validation(v).await;
                if let Err(e) = &result {
                    tracing::warn!(
                        target: "memex.qa",
                        stage = "memory.validate.error",
                        qa_id = %qa_id,
                        error = %e,
                        "Failed to record validation (non-fatal)"
                    );
                }
                tracing::info!(target: "memex.qa", stage = "memory.validate.out");
                results.push(result);
            }
            results
        };

        let candidates_future = async {
            if decision.should_write_candidate && !decision.candidate_drafts.is_empty() {
                let payloads = build_candidate_payloads(ctx.project_id, &decision.candidate_drafts);
                let mut results = Vec::new();
                for c in payloads {
                    tracing::debug!(
                        target: "memex.qa",
                        stage = "memory.candidate.in",
                        tags = c.tags.len()
                    );
                    let result = mem.record_candidate(c).await;
                    if let Err(e) = &result {
                        tracing::warn!(
                            target: "memex.qa",
                            stage = "memory.candidate.error",
                            error = %e,
                            error_debug = ?e,
                            "Failed to record candidate (non-fatal)"
                        );
                    }
                    tracing::debug!(target: "memex.qa", stage = "memory.candidate.out");
                    results.push(result);
                }
                results
            } else {
                Vec::new()
            }
        };

        // Execute all three operations in parallel
        let (_, _, _) = futures::join!(hit_future, validations_future, candidates_future);

        tracing::info!(
            target: "memex.qa",
            stage = "post.end",
            should_write_candidate = decision.should_write_candidate,
            inject = decision.inject_list.len(),
            hit_refs = decision.hit_refs.len(),
            validate_plans = decision.validate_plans.len(),
            candidate_drafts = decision.candidate_drafts.len()
        );
    }
    Ok((run_outcome, decision))
}
