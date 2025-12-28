use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;

use crate::config::ControlConfig;
use crate::error::RunnerError;
use crate::events_out::EventsOutTx;
use crate::tool_event::{CompositeToolEventParser, ToolEventRuntime, TOOL_EVENT_PREFIX};
use crate::util::RingBytes;

use super::tee;
use super::traits::{PolicyPlugin, RunnerSession};
use super::types::{PolicyAction, RunnerResult, Signal};

pub async fn run_session(
    mut session: Box<dyn RunnerSession>,
    control: &ControlConfig,
    policy: Option<Box<dyn PolicyPlugin>>,
    capture_bytes: usize,
    events_out: Option<EventsOutTx>,
    run_id: &str,
    silent: bool,
) -> Result<RunnerResult, RunnerError> {
    let _span = tracing::info_span!(
        "core.run_session",
        run_id = %run_id,
        capture_bytes = capture_bytes,
        silent = silent,
        fail_mode = %control.fail_mode,
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

    let (line_tx, mut line_rx) = mpsc::channel::<tee::LineTap>(control.line_tap_channel_capacity);
    let out_task = tee::pump_stdout(stdout, ring_out.clone(), line_tx.clone(), silent);
    let err_task = tee::pump_stderr(stderr, ring_err.clone(), line_tx, silent);

    let (ctl_tx, mut ctl_rx) = mpsc::channel::<serde_json::Value>(control.control_channel_capacity);
    let mut ctl = ControlChannel::new(stdin);
    let fail_closed = control.fail_mode.as_str() == "closed";

    let (writer_err_tx, mut writer_err_rx) =
        mpsc::channel::<String>(control.control_writer_error_capacity);
    let ctl_task = tokio::spawn(async move {
        while let Some(v) = ctl_rx.recv().await {
            if let Err(e) = ctl.send(&v).await {
                let _ = writer_err_tx
                    .send(format!("stdin write failed: {}", e))
                    .await;
                break;
            }
        }
    });

    let pending: HashMap<String, Instant> = HashMap::new();
    let decision_timeout = Duration::from_millis(control.decision_timeout_ms);
    let mut tick = tokio::time::interval(Duration::from_millis(control.tick_interval_ms));

    let parser = CompositeToolEventParser::new(TOOL_EVENT_PREFIX);
    let mut tool_runtime =
        ToolEventRuntime::new(parser, events_out.clone(), Some(run_id.to_string()));

    let (exit_status, abort_reason) = {
        let wait_fut = session.wait();
        tokio::pin!(wait_fut);

        let mut status = None;
        let mut reason = None;

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
                            reason = Some("control channel broken".to_string());
                            break;
                        } else {
                            tracing::warn!("control channel broken, continuing in fail-open mode");
                        }
                    }
                }

                tap = line_rx.recv() => {
                    if let Some(tap) = tap {
                        if let Some(ev) = tool_runtime.observe_line(&tap.line).await {
                            if ev.event_type == "tool.request" {
                                if let Some(p) = &policy {
                                    match p.check(&ev).await {
                                        PolicyAction::Deny { reason: r } => {
                                            tracing::error!(error.kind="policy.deny", tool=%ev.tool.as_deref().unwrap_or("?"), reason=%r);
                                            reason = Some(format!("policy denial: {}", r));
                                            break;
                                        }
                                        PolicyAction::Ask { prompt } => {
                                            tracing::warn!("Policy requested approval for tool {}, but interactive mode is not implemented. Denying by default.", ev.tool.as_deref().unwrap_or("?"));
                                            reason = Some(format!("policy requires approval: {}", prompt));
                                            break;
                                        }
                                        PolicyAction::Allow => {}
                                    }
                                }
                            }
                        }
                    }
                }

                _ = tick.tick() => {
                    let now = Instant::now();
                    let mut timed_out = false;
                    for (_, t0) in pending.iter() {
                        if now.duration_since(*t0) > decision_timeout {
                            timed_out = true;
                            break;
                        }
                    }
                    if timed_out {
                        tracing::error!(error.kind="control.decision_timeout");
                        if fail_closed {
                            reason = Some("decision timeout".to_string());
                            break;
                        }
                    }
                }
            }
        }
        (status, reason)
    };

    if let Some(reason) = abort_reason {
        let effective_run_id = tool_runtime.effective_run_id().unwrap_or(run_id);
        abort_sequence(
            &mut session,
            &ctl_tx,
            effective_run_id,
            control.abort_grace_ms,
            &reason,
        )
        .await;
        return Ok(RunnerResult {
            run_id: effective_run_id.to_string(),
            exit_code: 40,
            duration_ms: None,
            stdout_tail: String::new(),
            stderr_tail: String::new(),
            tool_events: vec![],
            dropped_lines: tool_runtime.dropped_events_out(),
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
    let start_time = Instant::now();

    let stdout_tail = String::from_utf8_lossy(&ring_out.to_bytes()).to_string();
    let stderr_tail = String::from_utf8_lossy(&ring_err.to_bytes()).to_string();

    let tool_events = tool_runtime.take_events();
    let dropped = tool_runtime.dropped_events_out();
    let effective_run_id = tool_runtime
        .effective_run_id()
        .unwrap_or(run_id)
        .to_string();

    Ok(RunnerResult {
        run_id: effective_run_id,
        exit_code,
        duration_ms: Some(start_time.elapsed().as_millis() as u64),
        stdout_tail,
        stderr_tail,
        tool_events,
        dropped_lines: dropped,
    })
}

async fn abort_sequence(
    session: &mut Box<dyn RunnerSession>,
    ctl_tx: &mpsc::Sender<serde_json::Value>,
    run_id: &str,
    abort_grace_ms: u64,
    reason: &str,
) {
    let abort = PolicyAbortCmd::new(
        run_id.to_string(),
        reason.to_string(),
        Some("policy_violation".into()),
    );
    let _ = ctl_tx.send(serde_json::to_value(abort).unwrap()).await;
    tokio::time::sleep(Duration::from_millis(abort_grace_ms)).await;
    let _ = session.signal(Signal::Kill).await;
}

pub struct ControlChannel {
    stdin: Box<dyn AsyncWrite + Unpin + Send>,
}

impl ControlChannel {
    pub fn new(stdin: Box<dyn AsyncWrite + Unpin + Send>) -> Self {
        Self { stdin }
    }

    pub async fn send<T: Serialize>(&mut self, msg: &T) -> std::io::Result<()> {
        let line = serde_json::to_string(msg).unwrap();
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await
    }
}

#[derive(Debug, Serialize)]
struct PolicyAbortCmd {
    pub v: u8,
    #[serde(rename = "type")]
    pub ty: &'static str,
    pub ts: String,
    pub run_id: String,
    pub id: String,
    pub reason: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

impl PolicyAbortCmd {
    pub fn new(run_id: String, reason: String, code: Option<String>) -> Self {
        let now = chrono::Utc::now();
        let id = format!("abort-{}-{}", run_id, now.timestamp_millis());
        Self {
            v: 1,
            ty: "control.abort",
            ts: now.to_rfc3339(),
            run_id,
            id,
            reason,
            code,
        }
    }
}
