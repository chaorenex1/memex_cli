//! HTTP路由handlers

use axum::{
    extract::State,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::Local;

use super::{
    models::*,
    state::AppState,
    validation::{validate_candidate, validate_project_id},
};
use axum::{body::Body, extract::Path, http::header, response::Response};
use bytes::Bytes;
use core_api::{
    post_run, pre_run, PreRun, QACandidatePayload, QAHitsPayload, QAReferencePayload,
    QAValidationPayload, WrapperEvent,
};
use memex_core::api as core_api;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

/// 创建所有路由
pub fn create_router(state: AppState) -> Router {
    Router::new()
        // CS 模式统一命令接口
        .route("/exec/:command", post(exec_handler))
        // Memory API（保留用于外部集成）
        .route("/api/v1/search", post(search_handler))
        .route("/api/v1/record-candidate", post(record_candidate_handler))
        .route("/api/v1/record-hit", post(record_hit_handler))
        .route("/api/v1/record-validation", post(record_validation_handler))
        .route("/api/v1/validate", post(validate_handler))
        .route("/api/v1/evaluate-session", post(evaluate_session_handler))
        // 系统接口
        .route("/health", get(health_handler))
        .route("/api/v1/shutdown", post(shutdown_handler))
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

    let pre = pre_run(
        &req.project_id,
        state.config.as_ref(),
        state.services.as_ref(),
        &req.query,
    )
    .await;

    Ok(Json(SearchResponse {
        success: true,
        data: Some(serde_json::json!({
            "merged_query": pre.merged_query,
            "shown_qa_ids": pre.shown_qa_ids,
            "matches": pre.matches,
        })),
        error: None,
        error_code: None,
    }))
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
        context: Some(serde_json::json!({"confidence": req.confidence})),
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
        context: req.context.map(|s| serde_json::json!({ "message": s })),
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

    // 验证（保持同步校验：无效 project_id 直接返回错误）
    validate_project_id(&req.project_id)?;

    // 后台执行（避免阻塞 HTTP 请求）
    let state_clone = state.clone();
    let req_clone = req.clone();

    tokio::spawn(async move {
        let EvaluateSessionRequest {
            project_id,
            session_id,
            user_query,
            matches,
            transcript_path,
            stdout,
            stderr,
            shown_qa_ids,
            used_qa_ids: _,
            exit_code,
            duration_ms,
        } = req_clone;

        let result: anyhow::Result<()> = async {
            let tool_events: Vec<core_api::ToolEvent> =
                core_api::StreamJsonToolEventParser::parse_transcript_path_file(&transcript_path)?;

            let run = core_api::RunnerResult {
                run_id: session_id.clone(),
                exit_code,
                duration_ms: Some(duration_ms),
                stdout_tail: stdout,
                stderr_tail: stderr,
                tool_events,
                dropped_lines: 0,
            };

            let mut ev =
                WrapperEvent::new("memory.search.result", chrono::Local::now().to_rfc3339());
            ev.data = Some(serde_json::json!({
                "query": user_query.clone(),
                "matches": matches.clone(),
            }));

            let pre = PreRun {
                merged_query: user_query.clone(),
                shown_qa_ids,
                matches,
                memory_search_event: Some(ev),
            };

            let events_out_tx = state_clone.ctx.events_out();
            let (_run_outcome, decision) = post_run(
                &run,
                &pre,
                &project_id,
                state_clone.config.as_ref(),
                state_clone.services.as_ref(),
                &events_out_tx,
                &user_query,
            )
            .await?;

            info!(
                target: "memex.http",
                "evaluate-session finished (session_id={}): should_write_candidate={}, hit_refs={}, validate_plans={}",
                session_id,
                decision.should_write_candidate,
                decision.hit_refs.len(),
                decision.validate_plans.len()
            );

            Ok(())
        }
        .await;

        if let Err(e) = result {
            let mut stats = state_clone.stats.write().unwrap();
            stats.increment_error();
            error!(
                target: "memex.http",
                "evaluate-session background task failed (project_id={}, session_id={}, transcript_path={}): {}",
                project_id,
                session_id,
                transcript_path,
                e
            );
        }
    });

    Ok(Json(EvaluateSessionResponse {
        success: true,
        decision_summary: Some(format!("scheduled (session_id={})", req.session_id)),
        candidates_recorded: 0,
        hits_recorded: 0,
        validations_recorded: 0,
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

/// POST /exec/{command} - 统一命令执行入口
///
/// 支持的命令：
/// - run: 执行查询（统一入口：run_multi_tasks）
/// - replay: 重放运行分析（直接调用 core::replay_cmd）
/// - search: 搜索记忆（直接调用 Memory Service）
pub async fn exec_handler(
    Path(command): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> impl IntoResponse {
    let command = command.to_lowercase();

    if !matches!(command.as_str(), "run" | "replay" | "search") {
        let error_msg = format!(
            "Unknown command: {}\nSupported: run, replay, search\n",
            command
        );
        return Response::builder()
            .status(400)
            .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(Body::from(error_msg))
            .unwrap()
            .into_response();
    }

    debug!(target: "memex.http", "Received exec command: {}", command);

    let wants_sse = req.get("sse").and_then(|v| v.as_bool()).unwrap_or(false);

    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();

    let state_clone = state.clone();
    let command_clone = command.clone();

    tokio::spawn(async move {
        info!(target: "memex.http", "Executing command: {}", command_clone);

        let result = match command_clone.as_str() {
            "run" => exec_run(&state_clone, &req, &wants_sse, &tx).await,
            _ => Err(anyhow::anyhow!("Unknown command")),
        };

        if let Err(e) = result {
            error!(target: "memex.http", "Command {} failed: {}", command_clone, e);
            let _ = tx.send(format!("Error: {}\n", e).into_bytes());
        }
    });

    let body = Body::from_stream(async_stream::stream! {
        while let Some(chunk) = rx.recv().await {
            yield Ok::<_, axum::Error>(Bytes::from(chunk));
        }
    });

    let resp = Response::builder()
        .status(200)
        .header(
            header::CONTENT_TYPE,
            if wants_sse {
                "text/event-stream; charset=utf-8"
            } else {
                "text/plain; charset=utf-8"
            },
        )
        .header("X-Accel-Buffering", "no")
        .header("Cache-Control", "no-cache");

    resp.body(body).unwrap().into_response()
}

/// 执行 run 命令 - 直接调用 API
async fn exec_run(
    state: &AppState,
    req: &serde_json::Value,
    wants_sse: &bool,
    tx: &mpsc::UnboundedSender<Vec<u8>>,
) -> anyhow::Result<()> {
    let stdio_opts: core_api::StdioRunOpts = req
        .get("options")
        .ok_or_else(|| anyhow::anyhow!("missing field: options"))
        .and_then(|v| serde_json::from_value(v.clone()).map_err(|e| anyhow::anyhow!(e)))?;

    let stdio_tasks: Vec<core_api::StdioTask> = req
        .get("tasks")
        .ok_or_else(|| anyhow::anyhow!("missing field: tasks"))
        .and_then(|v| serde_json::from_value(v.clone()).map_err(|e| anyhow::anyhow!(e)))?;

    let http_sse_tx = if *wants_sse { Some(tx.clone()) } else { None };

    let exit_code = crate::flow::flow_standard::run_multi_tasks(
        &stdio_tasks,
        &stdio_opts,
        &state.ctx,
        http_sse_tx,
    )
    .await
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    // Keep legacy marker for existing clients (e.g. RemoteClient) to parse.
    let _ = tx.send(format!("[Exit: {}]\n", exit_code).into_bytes());

    Ok(())
}
