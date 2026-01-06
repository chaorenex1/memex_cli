//! Runner runtime：负责 stdout/stderr 泵送、tool 事件解析、policy/timeout 控制，以及中止（abort）与退出码归一。
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use crate::config::ControlConfig;
use crate::error::RunnerError;
use crate::events_out::EventsOutTx;
use crate::util::RingBytes;

use super::abort;
use super::control;
use super::io_pump;
use super::output::{
    maybe_apply_policy, JsonlParser, OutputEvent, OutputSink, StdioSink, StreamParser, TextParser,
    TuiSink,
};
use super::policy::{PolicyEngine, PolicyOutcome};
use super::traits::{PolicyPlugin, RunnerSession};
use super::types::RunnerResult;
use super::RunnerEvent;
use tokio::io::AsyncWriteExt;

fn flow_audit_enabled() -> bool {
    std::env::var_os("MEMEX_FLOW_AUDIT")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false)
}

fn audit_preview(s: &str) -> String {
    const MAX: usize = 160;
    if s.len() <= MAX {
        return s.to_string();
    }
    let end = s
        .char_indices()
        .take_while(|(i, _)| *i < MAX)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    let mut out = s[..end].to_string();
    out.push('…');
    out
}

pub struct RunSessionRuntimeInput<'a> {
    pub session: Box<dyn RunnerSession>,
    pub control_cfg: &'a ControlConfig,
    pub policy: Option<Arc<dyn PolicyPlugin>>,
    pub capture_bytes: usize,
    pub events_out: Option<EventsOutTx>,
    pub event_tx: Option<mpsc::UnboundedSender<RunnerEvent>>,
    pub run_id: &'a str,
    pub backend_kind: &'a str,
    pub stream_format: &'a str,
    pub abort_rx: Option<mpsc::Receiver<String>>,
}

pub async fn run_session_runtime(
    input: RunSessionRuntimeInput<'_>,
) -> Result<RunnerResult, RunnerError> {
    let RunSessionRuntimeInput {
        mut session,
        control_cfg,
        policy,
        capture_bytes,
        events_out,
        event_tx: tui_tx,
        run_id,
        backend_kind,
        stream_format,
        mut abort_rx,
    } = input;
    let _span = tracing::info_span!(
        "core.run_session",
        run_id = %run_id,
        capture_bytes = capture_bytes,
        stream_format = %stream_format,
        backend_kind = %backend_kind,
        fail_mode = %control_cfg.fail_mode,
    );
    let _enter = _span.enter();

    let stdout = session
        .stdout()
        .ok_or_else(|| RunnerError::Spawn("no stdout".into()))?;
    let stderr = session
        .stderr()
        .ok_or_else(|| RunnerError::Spawn("no stderr".into()))?;
    let stdin = session
        .stdin()
        .ok_or_else(|| RunnerError::Spawn("no stdin".into()))?;

    let ring_out = RingBytes::new(capture_bytes);
    let ring_err = RingBytes::new(capture_bytes);

    let started_at = Instant::now();
    let flow_audit = flow_audit_enabled();
    if flow_audit {
        tracing::debug!(
            target: "memex.flow",
            stage = "runtime.start",
            run_id = %run_id,
            stream_format = %stream_format,
            backend_kind = %backend_kind
        );
    }

    let (line_tx, mut line_rx) =
        mpsc::channel::<io_pump::LineTap>(control_cfg.line_tap_channel_capacity);
    let out_task = io_pump::pump_stdout(stdout, ring_out.clone(), line_tx.clone());
    let err_task = io_pump::pump_stderr(stderr, ring_err.clone(), line_tx);

    let fail_closed = control_cfg.fail_mode.as_str() == "closed";

    // CodeCLI runner sessions are expected to be non-interactive.
    // Keeping stdin open (piped) can cause some CLIs to wait indefinitely for input.
    // Since codecli skips policy/control messages, close stdin immediately for this backend.
    let (ctl_tx, mut writer_err_rx, ctl_task) = if backend_kind == "codecli" {
        drop(stdin);
        let (ctl_tx, _ctl_rx) = mpsc::channel::<serde_json::Value>(1);
        let (_err_tx, err_rx) = mpsc::channel::<String>(1);
        drop(_ctl_rx);
        drop(_err_tx);
        let task = tokio::spawn(async move { Ok(()) });
        (ctl_tx, err_rx, task)
    } else {
        control::spawn_control_writer(
            stdin,
            control_cfg.control_channel_capacity,
            control_cfg.control_writer_error_capacity,
        )
    };

    let decision_timeout = Duration::from_millis(control_cfg.decision_timeout_ms);
    let mut tick = tokio::time::interval(Duration::from_millis(control_cfg.tick_interval_ms));

    let mut parser_kind = if stream_format == "jsonl" {
        ParserKind::Jsonl(JsonlParser::new(events_out.clone(), run_id))
    } else {
        ParserKind::Text(TextParser::new(events_out.clone(), run_id))
    };

    let mut sink_kind = if let Some(tx) = tui_tx.clone() {
        SinkKind::Tui(TuiSink::new(tx))
    } else {
        SinkKind::Stdio(StdioSink::new())
    };

    let mut policy_engine = PolicyEngine::new(fail_closed, decision_timeout);

    let (exit_status, abort_reason) = {
        let wait_fut = session.wait();
        tokio::pin!(wait_fut);

        let mut status = None;
        let mut reason: Option<(String, i32, Option<String>)> = None;

        async fn write_parent_stderr_line(line: &str) {
            let mut stderr = tokio::io::stderr();
            let _ = stderr.write_all(line.as_bytes()).await;
            let _ = stderr.write_all(b"\n").await;
            let _ = stderr.flush().await;
        }

        loop {
            tokio::select! {
                res = &mut wait_fut => {
                    status = Some(res);
                    break;
                }

                maybe_err = writer_err_rx.recv() => {
                    if let Some(msg) = maybe_err {
                        tracing::error!(error.kind="control.stdin_broken", error.message=%msg);
                        if fail_closed {
                            reason = Some(("control channel broken".to_string(), 40, None));
                            break;
                        } else {
                            tracing::warn!("control channel broken, continuing in fail-open mode");
                        }
                    }
                }

                abort_msg = async {
                    match abort_rx.as_mut() {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    if let Some(msg) = abort_msg {
                        tracing::warn!(error.kind="user.abort", reason=%msg);
                        reason = Some((msg, 130, Some("user_abort".into())));
                        break;
                    }
                }

                tap = line_rx.recv() => {
                    if let Some(tap) = tap {
                        if flow_audit {
                            tracing::debug!(
                                target: "memex.flow",
                                stage = "runtime.tap",
                                stream = ?tap.stream,
                                bytes = tap.line.len(),
                                preview = %audit_preview(&tap.line)
                            );
                        }
                        // Child stderr always bypasses parsing and is written directly to the parent stderr.
                        // The parser only ever handles child stdout.
                        if matches!(tap.stream, io_pump::LineStream::Stderr) {
                            if flow_audit {
                                tracing::debug!(target: "memex.flow", stage = "runtime.stderr_passthrough");
                            }
                            write_parent_stderr_line(&tap.line).await;
                            continue;
                        }

                        match parser_kind.parse(&tap).await {
                            Ok(events) => {
                                if flow_audit {
                                    tracing::debug!(
                                        target: "memex.flow",
                                        stage = "runtime.parsed",
                                        produced = events.len()
                                    );
                                }
                                for ev in events {
                                    if let OutputEvent::ToolEvent(ref tool_ev) = ev {
                                        if flow_audit {
                                            tracing::debug!(
                                                target: "memex.flow",
                                                stage = "policy.in",
                                                event_type = %tool_ev.event_type
                                            );
                                        }
                                        match maybe_apply_policy(
                                            backend_kind,
                                            &mut policy_engine,
                                            policy.as_deref(),
                                            &ctl_tx,
                                            run_id,
                                            tool_ev.as_ref(),
                                        )
                                        .await
                                        {
                                            PolicyOutcome::Continue => {}
                                            PolicyOutcome::Abort(r) => {
                                                tracing::error!(error.kind="policy.abort", reason=%r);
                                                reason = Some((r, 40, Some("policy_violation".into())));
                                                break;
                                            }
                                        }
                                        if flow_audit {
                                            tracing::debug!(target: "memex.flow", stage = "policy.out", outcome = "continue");
                                        }
                                    }

                                    if flow_audit {
                                        tracing::debug!(
                                            target: "memex.flow",
                                            stage = "sink.in",
                                            kind = match &ev {
                                                OutputEvent::RawLine {..} => "raw_line",
                                                OutputEvent::ToolEvent(_) => "tool_event",
                                            }
                                        );
                                    }
                                    sink_kind.emit(ev).await;
                                    if flow_audit {
                                        tracing::debug!(target: "memex.flow", stage = "sink.out");
                                    }
                                }
                            }
                            Err(e) => {
                                if flow_audit {
                                    tracing::debug!(
                                        target: "memex.flow",
                                        stage = "runtime.parse_error",
                                        reason = e.reason,
                                        stream = ?e.stream,
                                        preview = %e.line_preview
                                    );
                                }
                                match e.reason {
                                    "invalid_json" => tracing::error!(
                                        error.kind="stream.parse_failed",
                                        error.reason=e.reason,
                                        stream=?e.stream,
                                        line=%e.line_preview
                                    ),
                                    "non_json_line" => tracing::debug!(
                                        error.kind="stream.parse_skipped",
                                        error.reason=e.reason,
                                        stream=?e.stream,
                                        line=%e.line_preview
                                    ),
                                    _ => tracing::warn!(
                                        error.kind="stream.parse_failed",
                                        error.reason=e.reason,
                                        stream=?e.stream,
                                        line=%e.line_preview
                                    ),
                                }
                            }
                        }
                    }
                }

                _ = tick.tick() => {
                    let now = Instant::now();
                    match policy_engine.on_tick(now, &ctl_tx, run_id).await {
                        PolicyOutcome::Continue => {}
                        PolicyOutcome::Abort(r) => {
                            tracing::error!(error.kind="control.decision_timeout", reason=%r);
                            reason = Some((r, 40, Some("decision_timeout".into())));
                            break;
                        }
                    }
                }
            }
        }
        (status, reason)
    };

    if let Some((reason, exit_code, code)) = abort_reason {
        let effective_run_id = parser_kind.effective_run_id().unwrap_or(run_id);
        abort::abort_sequence(
            &mut session,
            &ctl_tx,
            effective_run_id,
            control_cfg.abort_grace_ms,
            &reason,
            code,
        )
        .await;
        let duration_ms = started_at.elapsed().as_millis() as u64;
        if let Some(tx) = &tui_tx {
            let _ = tx.send(RunnerEvent::Error(reason.clone()));
            let _ = tx.send(RunnerEvent::RunComplete { exit_code });
        }
        return Ok(RunnerResult {
            run_id: effective_run_id.to_string(),
            exit_code,
            duration_ms: Some(duration_ms),
            stdout_tail: String::new(),
            stderr_tail: String::new(),
            tool_events: vec![],
            dropped_lines: parser_kind.dropped_events_out(),
        });
    }

    drop(ctl_tx);
    ctl_task.abort();
    out_task.await.ok();
    err_task.await.ok();

    let outcome = exit_status
        .unwrap()
        .map_err(|e| RunnerError::Spawn(e.to_string()))?;
    let exit_code = outcome.exit_code;

    let stdout_tail = String::from_utf8_lossy(&ring_out.to_bytes()).to_string();
    let stderr_tail = String::from_utf8_lossy(&ring_err.to_bytes()).to_string();

    let tool_events = parser_kind.take_tool_events();
    let dropped = parser_kind.dropped_events_out();
    let effective_run_id = parser_kind.effective_run_id().unwrap_or(run_id).to_string();

    let duration_ms = started_at.elapsed().as_millis() as u64;

    if let Some(tx) = &tui_tx {
        let _ = tx.send(RunnerEvent::RunComplete { exit_code });
    }

    if flow_audit {
        tracing::debug!(
            target: "memex.flow",
            stage = "runtime.end",
            run_id = %effective_run_id,
            exit_code = exit_code,
            duration_ms = duration_ms,
            tool_events = tool_events.len()
        );
    }
    Ok(RunnerResult {
        run_id: effective_run_id,
        exit_code,
        duration_ms: Some(duration_ms),
        stdout_tail,
        stderr_tail,
        tool_events,
        dropped_lines: dropped,
    })
}

enum ParserKind {
    Text(TextParser),
    Jsonl(JsonlParser),
}

impl ParserKind {
    async fn parse(
        &mut self,
        tap: &io_pump::LineTap,
    ) -> Result<Vec<OutputEvent>, super::output::ParseError> {
        match self {
            ParserKind::Text(p) => p.parse(tap).await,
            ParserKind::Jsonl(p) => p.parse(tap).await,
        }
    }

    fn take_tool_events(&mut self) -> Vec<crate::tool_event::ToolEvent> {
        match self {
            ParserKind::Text(_) => vec![],
            ParserKind::Jsonl(p) => p.take_tool_events(),
        }
    }

    fn dropped_events_out(&self) -> u64 {
        match self {
            ParserKind::Text(_) => 0,
            ParserKind::Jsonl(p) => p.dropped_events_out(),
        }
    }

    fn effective_run_id(&self) -> Option<&str> {
        match self {
            ParserKind::Text(_) => None,
            ParserKind::Jsonl(p) => p.effective_run_id(),
        }
    }
}

enum SinkKind {
    Tui(TuiSink),
    Stdio(StdioSink),
}

impl SinkKind {
    async fn emit(&mut self, ev: OutputEvent) {
        match self {
            SinkKind::Tui(s) => s.emit(ev).await,
            SinkKind::Stdio(s) => s.emit(ev).await,
        }
    }
}
