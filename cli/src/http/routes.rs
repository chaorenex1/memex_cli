//! HTTP路由handlers

use anyhow::Error;
use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use chrono::Local;
use memex_core::api::{
    QACandidatePayload, QAHitsPayload, QAReferencePayload, QASearchPayload, QAValidationPayload,
};
use memex_plugins::memory::http_client::MemoryHttpError;

use crate::http::{
    models::*,
    state::AppState,
    validation::{validate_candidate, validate_project_id},
};
use tower_http::services::ServeDir;

/// 创建所有路由
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/search", post(search_handler))
        .route("/api/v1/record-candidate", post(record_candidate_handler))
        .route("/api/v1/record-hit", post(record_hit_handler))
        .route("/api/v1/record-validation", post(record_validation_handler))
        .route("/api/v1/validate", post(validate_handler))
        .route("/api/v1/evaluate-session", post(evaluate_session_handler))
        .route("/health", get(health_handler))
        .route("/api/v1/shutdown", post(shutdown_handler))
        .nest_service("/", ServeDir::new("static"))
        .with_state(state)
}

/// POST /api/v1/search - 搜索记忆
async fn search_handler(
    State(state): State<AppState>,
    Json(req): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, HttpServerError> {
    // 更新统计
    {
        let mut stats = state.stats.write().unwrap();
        stats.increment_request("/api/v1/search");
    }

    // 验证 project_id
    validate_project_id(&req.project_id)?;

    // 检查 memory 服务
    let memory =
        state.services.memory.as_ref().ok_or_else(|| {
            HttpServerError::MemoryService("Memory service not configured".into())
        })?;

    // 构建 payload
    let payload = QASearchPayload {
        project_id: req.project_id,
        query: req.query,
        limit: req.limit,
        min_score: req.min_score,
    };

    // 调用 memory 服务
    match memory.search(payload).await {
        Ok(results) => {
            let data = serde_json::to_value(results).unwrap_or_default();
            Ok(Json(SearchResponse {
                success: true,
                data: Some(data),
                error: None,
                error_code: None,
            }))
        }
        Err(e) => {
            let mut stats = state.stats.write().unwrap();
            stats.increment_error();
            Err(HttpServerError::MemoryService(e.to_string()))
        }
    }
}

/// POST /api/v1/record-candidate - 记录候选QA
async fn record_candidate_handler(
    State(state): State<AppState>,
    Json(req): Json<RecordCandidateRequest>,
) -> Result<Json<RecordCandidateResponse>, HttpServerError> {
    // 更新统计
    {
        let mut stats = state.stats.write().unwrap();
        stats.increment_request("/api/v1/record-candidate");
    }

    // 验证
    validate_project_id(&req.project_id)?;
    validate_candidate(&req.question, &req.answer)?;

    // 检查 memory 服务
    let memory =
        state.services.memory.as_ref().ok_or_else(|| {
            HttpServerError::MemoryService("Memory service not configured".into())
        })?;

    // 构建 payload
    let payload = QACandidatePayload {
        project_id: req.project_id,
        question: req.question,
        answer: req.answer,
        tags: vec![],
        confidence: 0.0,
        metadata: serde_json::json!({}),
        summary: None,
        source: None,
        author: None,
    };

    // 调用 memory 服务
    match memory.record_candidate(payload).await {
        Ok(_) => Ok(Json(RecordCandidateResponse {
            success: true,
            message: Some("Candidate recorded successfully".into()),
            error: None,
            error_code: None,
        })),
        Err(e) => {
            let mut stats = state.stats.write().unwrap();
            stats.increment_error();
            Err(HttpServerError::MemoryService(e.to_string()))
        }
    }
}

/// POST /api/v1/record-hit - 记录命中
async fn record_hit_handler(
    State(state): State<AppState>,
    Json(req): Json<RecordHitRequest>,
) -> Result<Json<RecordHitResponse>, HttpServerError> {
    // 更新统计
    {
        let mut stats = state.stats.write().unwrap();
        stats.increment_request("/api/v1/record-hit");
    }

    // 验证
    validate_project_id(&req.project_id)?;

    // 检查 memory 服务
    let memory =
        state.services.memory.as_ref().ok_or_else(|| {
            HttpServerError::MemoryService("Memory service not configured".into())
        })?;

    // 构建 references（合并 qa_ids 和 shown_ids）
    let mut references = Vec::new();

    // qa_ids 标记为 used=true
    for qa_id in req.qa_ids {
        references.push(QAReferencePayload {
            qa_id,
            shown: None,
            used: Some(true),
            message_id: None,
            context: None,
        });
    }

    // shown_ids 标记为 shown=true
    if let Some(shown_ids) = req.shown_ids {
        for qa_id in shown_ids {
            // 检查是否已经在 references 中
            if !references.iter().any(|r| r.qa_id == qa_id) {
                references.push(QAReferencePayload {
                    qa_id,
                    shown: Some(true),
                    used: None,
                    message_id: None,
                    context: None,
                });
            }
        }
    }

    // 构建 payload
    let payload = QAHitsPayload {
        project_id: req.project_id,
        references,
    };

    // 调用 memory 服务
    match memory.record_hit(payload).await {
        Ok(_) => Ok(Json(RecordHitResponse {
            success: true,
            data: Some(serde_json::json!({"message": "Hit recorded successfully"})),
            error: None,
            error_code: None,
        })),
        Err(e) => {
            let mut stats = state.stats.write().unwrap();
            stats.increment_error();
            Err(HttpServerError::MemoryService(e.to_string()))
        }
    }
}

/// POST /api/v1/record-validation - 记录QA验证结果
async fn record_validation_handler(
    State(state): State<AppState>,
    Json(req): Json<RecordValidationRequest>,
) -> Result<Json<RecordValidationResponse>, HttpServerError> {
    // 更新统计
    {
        let mut stats = state.stats.write().unwrap();
        stats.increment_request("/api/v1/record-validation");
    }

    // 验证
    validate_project_id(&req.project_id)?;

    // 检查 memory 服务
    let memory =
        state.services.memory.as_ref().ok_or_else(|| {
            HttpServerError::MemoryService("Memory service not configured".into())
        })?;

    // 构建 validation payload
    let payload = QAValidationPayload {
        project_id: req.project_id.clone(),
        qa_id: req.qa_id.clone(),
        result: None,
        signal_strength: None,
        success: Some(req.success),
        strong_signal: Some(req.success && req.confidence >= 0.8),
        source: Some("http-api".to_string()),
        context: Some(format!("confidence:{}", req.confidence)),
        client: None,
        ts: Some(Local::now().to_rfc3339()),
        payload: None,
    };

    // 调用 memory 服务
    match memory.record_validation(payload).await {
        Ok(_) => Ok(Json(RecordValidationResponse {
            success: true,
            message: Some(format!(
                "Validation recorded for {} (success={}, confidence={})",
                req.qa_id, req.success, req.confidence
            )),
            error: None,
            error_code: None,
        })),
        Err(e) => {
            let mut stats = state.stats.write().unwrap();
            stats.increment_error();
            Err(HttpServerError::MemoryService(e.to_string()))
        }
    }
}

/// POST /api/v1/validate - 记录验证
async fn validate_handler(
    State(state): State<AppState>,
    Json(req): Json<ValidateRequest>,
) -> Result<Json<ValidateResponse>, HttpServerError> {
    // 更新统计
    {
        let mut stats = state.stats.write().unwrap();
        stats.increment_request("/api/v1/validate");
    }

    // 验证
    validate_project_id(&req.project_id)?;

    // 验证 result 字段
    if req.result != "success" && req.result != "fail" {
        return Err(HttpServerError::InvalidRequest(
            "result must be 'success' or 'fail'".into(),
        ));
    }

    // 检查 memory 服务
    let memory =
        state.services.memory.as_ref().ok_or_else(|| {
            HttpServerError::MemoryService("Memory service not configured".into())
        })?;

    // 转换 result 和 signal_strength
    let success = req.result == "success";
    let strong_signal = req.signal_strength.as_ref().map(|s| s.as_str() == "strong");

    // 构建 payload
    let payload = QAValidationPayload {
        project_id: req.project_id,
        qa_id: req.qa_id,
        result: Some(req.result),
        signal_strength: req.signal_strength,
        success: Some(success),
        strong_signal,
        source: None,
        context: req.context,
        client: None,
        ts: None,
        payload: req.payload,
    };

    // 调用 memory 服务
    match memory.record_validation(payload).await {
        Ok(_) => Ok(Json(ValidateResponse {
            success: true,
            error: None,
            error_code: None,
        })),
        Err(e) => {
            let mut stats = state.stats.write().unwrap();
            stats.increment_error();
            Err(HttpServerError::MemoryService(e.to_string()))
        }
    }
}

/// GET /health - 健康检查
async fn health_handler(State(state): State<AppState>) -> Json<HealthResponse> {
    let stats = state.stats.read().unwrap();

    Json(HealthResponse {
        status: "healthy".into(),
        session_id: state.session_id.clone(),
        uptime_seconds: stats.uptime_seconds(),
        requests_handled: stats.requests_total,
        timestamp: Local::now().to_rfc3339(),
    })
}

/// POST /api/v1/evaluate-session - 评估会话并智能记录
async fn evaluate_session_handler(
    State(state): State<AppState>,
    Json(req): Json<EvaluateSessionRequest>,
) -> Result<Json<EvaluateSessionResponse>, HttpServerError> {
    // 更新统计
    {
        let mut stats = state.stats.write().unwrap();
        stats.increment_request("/api/v1/evaluate-session");
    }

    // 验证
    validate_project_id(&req.project_id)?;

    // 检查服务
    let memory =
        state.services.memory.as_ref().ok_or_else(|| {
            HttpServerError::MemoryService("Memory service not configured".into())
        })?;

    // 1. 不搜索 memory（避免与 pre-run 时的结果不一致）
    // 注意：这意味着智能抑制策略（has_strong, skip_if_top1_score_ge）会被禁用
    // 但在 Stop hook 场景下，pre-run 已经完成，shown_qa_ids 已经在 RunOutcome 中
    // Gatekeeper 将主要基于 exit_code 和 shown/used QA IDs 做决策
    let matches = vec![];

    // 2. 构建 RunOutcome
    use memex_core::api::RunOutcome;
    let run_outcome = RunOutcome {
        exit_code: req.exit_code,
        duration_ms: Some(req.duration_ms),
        stdout_tail: req.stdout.clone(),
        stderr_tail: req.stderr.clone(),
        tool_events: vec![], // 暂时为空，gatekeeper只使用exit_code和stdout
        shown_qa_ids: req.shown_qa_ids.clone(),
        used_qa_ids: req.used_qa_ids.clone(),
    };

    // 3. 构建 ToolEvent[] (从简化的ToolEventSimple转换)
    use memex_core::api::ToolEvent;
    let tool_events: Vec<ToolEvent> = req
        .tool_events
        .iter()
        .enumerate()
        .map(|(idx, te)| ToolEvent {
            v: 1,
            event_type: "tool".to_string(),
            ts: None,
            run_id: None,
            id: Some(format!("tool-{}", idx)),
            tool: Some(te.tool.clone()),
            action: None,
            args: te.args.clone(),
            ok: te.code.map(|c| c == 0),
            output: te
                .output
                .as_ref()
                .map(|s| serde_json::Value::String(s.clone())),
            error: None,
            rationale: None,
        })
        .collect();

    // 4. 读取 gatekeeper 配置（从 AppState 中获取）
    use memex_core::api::GatekeeperConfig;
    let gatekeeper_config = match &state.config.gatekeeper.provider {
        memex_core::api::GatekeeperProvider::Standard(cfg) => GatekeeperConfig {
            max_inject: cfg.max_inject,
            min_level_inject: cfg.min_level_inject,
            min_level_fallback: cfg.min_level_fallback,
            min_trust_show: cfg.min_trust_show,
            block_if_consecutive_fail_ge: cfg.block_if_consecutive_fail_ge,
            skip_if_top1_score_ge: cfg.skip_if_top1_score_ge,
            exclude_stale_by_default: cfg.exclude_stale_by_default,
            active_statuses: cfg.active_statuses.clone(),
            digest_head_chars: cfg.digest_head_chars,
            digest_tail_chars: cfg.digest_tail_chars,
        },
    };

    // 5. 调用 gatekeeper.evaluate()
    use memex_core::api::Gatekeeper;
    let decision = Gatekeeper::evaluate(
        &gatekeeper_config,
        Local::now(),
        &matches,
        &run_outcome,
        &tool_events,
    );

    tracing::info!(
        target: "memex.http",
        "Gatekeeper decision: should_write_candidate={}, hit_refs={}, validate_plans={}",
        decision.should_write_candidate,
        decision.hit_refs.len(),
        decision.validate_plans.len()
    );

    // 6. 记录 Hit
    let mut hits_recorded = 0;
    if !decision.hit_refs.is_empty() {
        use memex_core::api::build_hit_payload;
        if let Some(hit_payload) = build_hit_payload(&req.project_id, &decision) {
            match memory.record_hit(hit_payload).await {
                Ok(_) => {
                    hits_recorded = decision.hit_refs.len();
                    tracing::info!(target: "memex.http", "Recorded {} hits", hits_recorded);
                }
                Err(e) => {
                    let error_chain = format_error_chain(&e);
                    let (error_class, error_status, error_url) = memory_error_class(&e);
                    tracing::warn!(
                        target: "memex.http",
                        error = %e,
                        error_class = %error_class,
                        error_status = ?error_status,
                        error_url = ?error_url,
                        error_chain = %error_chain,
                        "Failed to record hits"
                    );
                }
            }
        }
    }

    // 7. 记录 Validation
    let mut validations_recorded = 0;
    use memex_core::api::build_validate_payloads;
    let validate_payloads = build_validate_payloads(&req.project_id, &decision);
    for payload in validate_payloads {
        match memory.record_validation(payload).await {
            Ok(_) => validations_recorded += 1,
            Err(e) => {
                let error_chain = format_error_chain(&e);
                let (error_class, error_status, error_url) = memory_error_class(&e);
                tracing::warn!(
                    target: "memex.http",
                    error = %e,
                    error_class = %error_class,
                    error_status = ?error_status,
                    error_url = ?error_url,
                    error_chain = %error_chain,
                    "Failed to record validation"
                );
            }
        }
    }

    // 8. 记录 Candidate
    let mut candidates_recorded = 0;
    if decision.should_write_candidate {
        // 从 stdout 提取候选答案
        use memex_core::api::ToolEventLite;
        use memex_core::api::{
            build_candidate_payloads, extract_candidates, CandidateExtractConfig,
        };

        // 从 AppState 中获取 candidate extract 配置
        let extract_config = CandidateExtractConfig {
            max_candidates: state.config.candidate_extract.max_candidates,
            max_answer_chars: state.config.candidate_extract.max_answer_chars,
            min_answer_chars: state.config.candidate_extract.min_answer_chars,
            context_lines: state.config.candidate_extract.context_lines,
            tool_steps_max: state.config.candidate_extract.tool_steps_max,
            tool_step_args_keys_max: state.config.candidate_extract.tool_step_args_keys_max,
            tool_step_value_max_chars: state.config.candidate_extract.tool_step_value_max_chars,
            redact: state.config.candidate_extract.redact,
            strict_secret_block: state.config.candidate_extract.strict_secret_block,
            confidence: state.config.candidate_extract.confidence,
        };

        let tool_events_lite: Vec<ToolEventLite> = tool_events.iter().map(|e| e.into()).collect();
        let candidate_drafts = extract_candidates(
            &extract_config,
            &req.user_query,
            &req.stdout,
            &req.stderr,
            &tool_events_lite,
        );

        let candidate_payloads = build_candidate_payloads(&req.project_id, &candidate_drafts);
        for candidate_payload in candidate_payloads {
            match memory.record_candidate(candidate_payload).await {
                Ok(_) => candidates_recorded += 1,
                Err(e) => {
                    let error_chain = format_error_chain(&e);
                    let (error_class, error_status, error_url) = memory_error_class(&e);
                    tracing::warn!(
                        target: "memex.http",
                        error = %e,
                        error_class = %error_class,
                        error_status = ?error_status,
                        error_url = ?error_url,
                        error_chain = %error_chain,
                        "Failed to record candidate"
                    );
                }
            }
        }
    }

    // 9. 返回响应
    Ok(Json(EvaluateSessionResponse {
        success: true,
        decision_summary: Some(decision.reasons.join("; ")),
        candidates_recorded,
        hits_recorded,
        validations_recorded,
        error: None,
        error_code: None,
    }))
}

/// POST /api/v1/shutdown - 触发优雅关闭
async fn shutdown_handler(State(state): State<AppState>) -> Json<serde_json::Value> {
    // 发送关闭信号
    let _ = state.shutdown_tx.send(());

    Json(serde_json::json!({
        "success": true,
        "message": "Shutdown signal sent"
    }))
}

fn memory_error_class(err: &Error) -> (String, Option<u16>, Option<String>) {
    for cause in err.chain() {
        if let Some(mem_err) = cause.downcast_ref::<MemoryHttpError>() {
            return (
                mem_err.kind().to_string(),
                mem_err.status(),
                mem_err.url().map(|url| url.to_string()),
            );
        }
    }

    ("unknown".to_string(), None, None)
}

fn format_error_chain(err: &Error) -> String {
    err.chain()
        .map(|cause| cause.to_string())
        .collect::<Vec<_>>()
        .join(" -> ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use memex_core::api::{MemoryPlugin, SearchMatch, Services, TaskGradeResult};
    use std::sync::Arc;
    use tokio::sync::broadcast;

    // Mock MemoryPlugin
    struct MockMemoryPlugin {
        should_fail: bool,
    }

    #[async_trait]
    impl MemoryPlugin for MockMemoryPlugin {
        fn name(&self) -> &str {
            "mock"
        }

        async fn search(&self, _payload: QASearchPayload) -> anyhow::Result<Vec<SearchMatch>> {
            if self.should_fail {
                anyhow::bail!("Mock search error")
            } else {
                Ok(vec![SearchMatch {
                    qa_id: "test-qa-id".into(),
                    project_id: Some("test-project".into()),
                    question: "Test question".into(),
                    answer: "Test answer".into(),
                    tags: vec![],
                    score: 0.95,
                    relevance: 0.95,
                    validation_level: 1,
                    level: Some("L1".into()),
                    trust: 0.9,
                    freshness: 1.0,
                    confidence: 0.95,
                    status: "active".into(),
                    summary: None,
                    source: None,
                    expiry_at: None,
                    metadata: serde_json::Value::Null,
                }])
            }
        }

        async fn record_hit(&self, _payload: QAHitsPayload) -> anyhow::Result<()> {
            if self.should_fail {
                anyhow::bail!("Mock record_hit error")
            } else {
                Ok(())
            }
        }

        async fn record_candidate(&self, _payload: QACandidatePayload) -> anyhow::Result<()> {
            if self.should_fail {
                anyhow::bail!("Mock record_candidate error")
            } else {
                Ok(())
            }
        }

        async fn record_validation(&self, _payload: QAValidationPayload) -> anyhow::Result<()> {
            if self.should_fail {
                anyhow::bail!("Mock record_validation error")
            } else {
                Ok(())
            }
        }

        async fn task_grade(&self, _prompt: String) -> anyhow::Result<TaskGradeResult> {
            Ok(TaskGradeResult {
                task_level: "L1".into(),
                reason: "Mock grade".into(),
                recommended_model: "gpt-4".into(),
                recommended_model_provider: Some("openai".into()),
                confidence: 0.9,
            })
        }
    }

    fn create_test_state(with_memory: bool, should_fail: bool) -> AppState {
        let (shutdown_tx, _) = broadcast::channel(1);
        let memory: Option<Arc<dyn MemoryPlugin>> = if with_memory {
            Some(Arc::new(MockMemoryPlugin { should_fail }))
        } else {
            None
        };

        let gatekeeper_config = memex_core::api::GatekeeperConfig::default();
        let services = Services {
            policy: None,
            memory,
            gatekeeper: Arc::new(memex_plugins::gatekeeper::StandardGatekeeperPlugin::new(
                gatekeeper_config,
            )),
        };

        // Create minimal test config
        let config = memex_core::api::AppConfig::default();

        AppState::new("test-session".into(), services, config, shutdown_tx)
    }

    #[tokio::test]
    async fn test_search_handler_success() {
        let state = create_test_state(true, false);
        let req = SearchRequest {
            query: "test query".into(),
            project_id: "test-project".into(),
            limit: 5,
            min_score: 0.6,
        };

        let result = search_handler(State(state.clone()), Json(req)).await;
        assert!(result.is_ok());

        let response = result.unwrap().0;
        assert!(response.success);
        assert!(response.data.is_some());
        assert!(response.error.is_none());

        // 检查统计
        let stats = state.stats.read().unwrap();
        assert_eq!(stats.requests_total, 1);
    }

    #[tokio::test]
    async fn test_search_handler_no_memory() {
        let state = create_test_state(false, false);
        let req = SearchRequest {
            query: "test query".into(),
            project_id: "test-project".into(),
            limit: 5,
            min_score: 0.6,
        };

        let result = search_handler(State(state), Json(req)).await;
        assert!(result.is_err());

        match result {
            Err(HttpServerError::MemoryService(msg)) => {
                assert!(msg.contains("not configured"));
            }
            _ => panic!("Expected MemoryService error"),
        }
    }

    #[tokio::test]
    async fn test_search_handler_invalid_project_id() {
        let state = create_test_state(true, false);
        let req = SearchRequest {
            query: "test query".into(),
            project_id: "invalid@project".into(),
            limit: 5,
            min_score: 0.6,
        };

        let result = search_handler(State(state), Json(req)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_record_candidate_handler_success() {
        let state = create_test_state(true, false);
        let req = RecordCandidateRequest {
            project_id: "test-project".into(),
            question: "What is Rust?".into(),
            answer: "Rust is a systems programming language".into(),
        };

        let result = record_candidate_handler(State(state.clone()), Json(req)).await;
        assert!(result.is_ok());

        let response = result.unwrap().0;
        assert!(response.success);
        assert!(response.message.is_some());
    }

    #[tokio::test]
    async fn test_record_candidate_handler_validation_error() {
        let state = create_test_state(true, false);
        let req = RecordCandidateRequest {
            project_id: "test-project".into(),
            question: "Hi".into(), // Too short
            answer: "Rust is a systems programming language".into(),
        };

        let result = record_candidate_handler(State(state), Json(req)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_validate_handler_success() {
        let state = create_test_state(true, false);
        let req = ValidateRequest {
            project_id: "test-project".into(),
            qa_id: "test-qa-id".into(),
            result: "success".into(),
            signal_strength: Some("strong".into()),
            context: None,
            payload: None,
        };

        let result = validate_handler(State(state), Json(req)).await;
        assert!(result.is_ok());

        let response = result.unwrap().0;
        assert!(response.success);
    }

    #[tokio::test]
    async fn test_validate_handler_invalid_result() {
        let state = create_test_state(true, false);
        let req = ValidateRequest {
            project_id: "test-project".into(),
            qa_id: "test-qa-id".into(),
            result: "invalid".into(),
            signal_strength: None,
            context: None,
            payload: None,
        };

        let result = validate_handler(State(state), Json(req)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_health_handler() {
        let state = create_test_state(true, false);
        let response = health_handler(State(state.clone())).await;

        assert_eq!(response.0.status, "healthy");
        assert_eq!(response.0.session_id, "test-session");
        assert!(response.0.uptime_seconds >= 0.0);
    }

    #[tokio::test]
    async fn test_shutdown_handler() {
        let state = create_test_state(true, false);
        let mut shutdown_rx = state.shutdown_tx.subscribe();

        let response = shutdown_handler(State(state)).await;
        assert_eq!(response.0["success"], true);

        // 验证关闭信号已发送
        let result = shutdown_rx.try_recv();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_record_hit_handler_success() {
        let state = create_test_state(true, false);
        let req = RecordHitRequest {
            project_id: "test-project".into(),
            qa_ids: vec!["qa1".into(), "qa2".into()],
            shown_ids: Some(vec!["qa3".into()]),
        };

        let result = record_hit_handler(State(state), Json(req)).await;
        assert!(result.is_ok());

        let response = result.unwrap().0;
        assert!(response.success);
    }

    #[tokio::test]
    async fn test_record_validation_handler_success() {
        let state = create_test_state(true, false);
        let req = RecordValidationRequest {
            project_id: "test-project".into(),
            qa_id: "qa-123".into(),
            success: true,
            confidence: 0.9,
        };

        let result = record_validation_handler(State(state), Json(req)).await;
        assert!(result.is_ok());

        let response = result.unwrap().0;
        assert!(response.success);
        assert!(response.message.is_some());
    }

    #[tokio::test]
    async fn test_record_validation_handler_no_memory() {
        let state = create_test_state(false, false);
        let req = RecordValidationRequest {
            project_id: "test-project".into(),
            qa_id: "qa-123".into(),
            success: true,
            confidence: 0.9,
        };

        let result = record_validation_handler(State(state), Json(req)).await;
        assert!(result.is_err());

        match result {
            Err(HttpServerError::MemoryService(msg)) => {
                assert!(msg.contains("not configured"));
            }
            _ => panic!("Expected MemoryService error"),
        }
    }
}
