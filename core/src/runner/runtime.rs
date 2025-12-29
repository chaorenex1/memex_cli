use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use crate::config::ControlConfig;
use crate::error::RunnerError;
use crate::events_out::EventsOutTx;
use crate::state::StateManager;
use crate::util::RingBytes;

use super::abort;
use super::control;
use super::io_pump;
use super::io_pump::LineStream;
use super::observe::ToolObserver;
use super::policy::{PolicyEngine, PolicyOutcome};
use super::state_report::StateReporter;
use super::traits::{PolicyPlugin, RunnerSession};
use super::types::RunnerResult;
use super::RunnerEvent;

pub struct RunSessionRuntimeInput<'a> {
    pub session: Box<dyn RunnerSession>,
    pub control_cfg: &'a ControlConfig,
    pub policy: Option<Box<dyn PolicyPlugin>>,
    pub capture_bytes: usize,
    pub events_out: Option<EventsOutTx>,
    pub event_tx: Option<mpsc::UnboundedSender<RunnerEvent>>,
    pub run_id: &'a str,
    pub silent: bool,
    pub state_manager: Option<Arc<StateManager>>,
    pub session_id: Option<String>,
}

pub async fn run_session_runtime(input: RunSessionRuntimeInput<'_>) -> Result<RunnerResult, RunnerError> {
    let RunSessionRuntimeInput {
        mut session,
        control_cfg,
        policy,
        capture_bytes,
        events_out,
        event_tx: tui_tx,
        run_id,
        silent,
        state_manager,
        session_id,
    } = input;
    let _span = tracing::info_span!(
        "core.run_session",
        run_id = %run_id,
        session_id = %session_id.as_deref().unwrap_or(""),
        capture_bytes = capture_bytes,
        silent = silent,
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

    let (line_tx, mut line_rx) =
        mpsc::channel::<io_pump::LineTap>(control_cfg.line_tap_channel_capacity);
    let out_task = io_pump::pump_stdout(stdout, ring_out.clone(), line_tx.clone(), silent);
    let err_task = io_pump::pump_stderr(stderr, ring_err.clone(), line_tx, silent);

    let fail_closed = control_cfg.fail_mode.as_str() == "closed";

    let (ctl_tx, mut writer_err_rx, ctl_task) = control::spawn_control_writer(
        stdin,
        control_cfg.control_channel_capacity,
        control_cfg.control_writer_error_capacity,
    );

    let decision_timeout = Duration::from_millis(control_cfg.decision_timeout_ms);
    let mut tick = tokio::time::interval(Duration::from_millis(control_cfg.tick_interval_ms));

    let mut tool_observer = ToolObserver::new(events_out.clone(), run_id);
    let mut state_reporter = StateReporter::new(state_manager, session_id);
    let mut policy_engine = PolicyEngine::new(fail_closed, decision_timeout);

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
                        if let Some(tx) = &tui_tx {
                            match tap.stream {
                                LineStream::Stdout => {
                                    let _ = tx.send(RunnerEvent::RawStdout(tap.line.clone()));
                                }
                                LineStream::Stderr => {
                                    let _ = tx.send(RunnerEvent::RawStderr(tap.line.clone()));
                                }
                            }
                        }

                        let tool_event = tool_observer.observe_line(&tap.line).await;
                        if let Some(ev) = tool_event {
                            state_reporter.on_tool_event();
                            if let Some(tx) = &tui_tx {
                                let _ = tx.send(RunnerEvent::ToolEvent(Box::new(ev.clone())));
                            }
                            if ev.event_type == "tool.request" {
                                match policy_engine
                                    .on_tool_request(&ev, policy.as_deref(), &ctl_tx, run_id)
                                    .await
                                {
                                    PolicyOutcome::Continue => {}
                                    PolicyOutcome::Abort(r) => {
                                        tracing::error!(error.kind="policy.abort", reason=%r);
                                        reason = Some(r);
                                        break;
                                    }
                                }
                            }
                        } else if matches!(tap.stream, LineStream::Stdout) {
                            if let Some(tx) = &tui_tx {
                                let _ = tx.send(RunnerEvent::AssistantOutput(tap.line.clone()));
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
                            reason = Some(r);
                            break;
                        }
                    }
                }
            }
        }
        (status, reason)
    };

    if let Some(reason) = abort_reason {
        let effective_run_id = tool_observer.effective_run_id().unwrap_or(run_id);
        abort::abort_sequence(
            &mut session,
            &ctl_tx,
            effective_run_id,
            control_cfg.abort_grace_ms,
            &reason,
        )
        .await;
        let duration_ms = started_at.elapsed().as_millis() as u64;
        state_reporter.set_runner_duration_ms(duration_ms).await;
        if let Some(tx) = &tui_tx {
            let _ = tx.send(RunnerEvent::Error(reason.clone()));
            let _ = tx.send(RunnerEvent::RunComplete { exit_code: 40 });
        }
        return Ok(RunnerResult {
            run_id: effective_run_id.to_string(),
            exit_code: 40,
            duration_ms: Some(duration_ms),
            stdout_tail: String::new(),
            stderr_tail: String::new(),
            tool_events: vec![],
            dropped_lines: tool_observer.dropped_events_out(),
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

    let tool_events = tool_observer.take_events();
    let dropped = tool_observer.dropped_events_out();
    let effective_run_id = tool_observer
        .effective_run_id()
        .unwrap_or(run_id)
        .to_string();

    let duration_ms = started_at.elapsed().as_millis() as u64;
    state_reporter.set_runner_duration_ms(duration_ms).await;

    if let Some(tx) = &tui_tx {
        let _ = tx.send(RunnerEvent::RunComplete { exit_code });
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
