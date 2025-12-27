mod codecli;
mod tee;
mod control;

use crate::{cli::Args, error::RunnerError};
use tokio::sync::mpsc;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::protocol::tool_event::{JsonlToolEventParser, ToolEvent};
use crate::protocol::policy_cmd::{PolicyDecisionCmd, PolicyAbortCmd};

pub async fn run(args: Args) -> Result<i32, RunnerError> {
    let cfg = crate::config::load_default().map_err(|e| RunnerError::Spawn(e.to_string()))?;

    let memory = if cfg.memory.enabled && !cfg.memory.base_url.trim().is_empty() {
        Some(crate::memory::MemoryClient::new(
            cfg.memory.base_url.clone(),
            cfg.memory.api_key.clone(),
            cfg.memory.timeout_ms,
        ).map_err(|e| RunnerError::Spawn(e.to_string()))?)
    } else { None };


    if let Some(mem) = &memory {
        let payload = crate::memory::QASearchPayload {
            project_id: cfg.project_id.clone(),
            query: "bootstrap query".to_string(),
            limit: cfg.memory.search_limit,
            min_score: cfg.memory.min_score,
        };
        if let Ok(v) = mem.search(payload).await {
            tracing::info!(action="memory.search", resp=%v);
        }
    }



    let mut child = codecli::spawn(&args)?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let stdin  = child.stdin.take().unwrap();

    let ring_out = crate::util::ring::RingBytes::new(args.capture_bytes);
    let ring_err = crate::util::ring::RingBytes::new(args.capture_bytes);

    // line taps
    let (line_tx, mut line_rx) = mpsc::channel::<tee::LineTap>(1024);
    let out_task = tee::pump_stdout(stdout, ring_out.clone(), line_tx.clone());
    let err_task = tee::pump_stderr(stderr, ring_err.clone(), line_tx);

    // control channel single writer
    let (ctl_tx, mut ctl_rx) = mpsc::channel::<serde_json::Value>(128);
    let run_id = uuid::Uuid::new_v4().to_string();

    let mut ctl = control::ControlChannel::new(stdin);
    let fail_closed = cfg.control.fail_mode.as_str() == "closed";

    // writer task：失败要上报
    let (writer_err_tx, mut writer_err_rx) = mpsc::channel::<String>(1);
    let ctl_task = tokio::spawn(async move {
        while let Some(v) = ctl_rx.recv().await {
            if let Err(e) = ctl.send(&v).await {
                let _ = writer_err_tx.send(format!("stdin write failed: {}", e)).await;
                break;
            }
        }
    });

    let policy = crate::policy::PolicyEngine::new(cfg.policy.clone());

    // pending decisions: id -> created_at
    let mut pending: HashMap<String, Instant> = HashMap::new();
    let decision_timeout = Duration::from_millis(cfg.control.decision_timeout_ms);

    // 监控计时器：每 1s 检查 pending 超时
    let mut tick = tokio::time::interval(Duration::from_millis(1000));

    // 主循环：同时处理 line_rx、writer_err、timeout、child exit
    let wait_fut = child.wait();
    tokio::pin!(wait_fut);

    loop {
        tokio::select! {
            // 子进程退出
            status = &mut wait_fut => {
                let status = status.map_err(|e| RunnerError::Spawn(e.to_string()))?;
                // 收尾
                drop(ctl_tx);
                ctl_task.abort();
                out_task.abort();
                err_task.abort();
                return Ok(normalize_exit(status));
            }

            // stdin 写失败
            maybe_err = writer_err_rx.recv() => {
                if let Some(msg) = maybe_err {
                    tracing::error!(error.kind="control.stdin_broken", error.message=%msg);
                    if fail_closed {
                        abort_sequence(&mut child, &ctl_tx, &run_id, cfg.control.abort_grace_ms, "control channel broken").await;
                        return Ok(40);
                    } else {
                        // fail-open：只告警，不中止
                        tracing::warn!("control channel broken, continuing in fail-open mode");
                    }
                }
            }

            // line 事件
            tap = line_rx.recv() => {
                if let Some(tap) = tap {
                    if let Ok(Some(evt)) = JsonlToolEventParser::parse_line(&tap.line) {
                        if let ToolEvent::Request(req) = evt {
                            let requires = req.requires_policy.unwrap_or(false);
                            if !requires { continue; }

                            pending.insert(req.id.clone(), Instant::now());

                            let d = policy.decide(&req);
                            let cmd = if d.decision == "allow" {
                                PolicyDecisionCmd::allow(run_id.clone(), req.id.clone(), d.reason, d.rule_id)
                            } else {
                                PolicyDecisionCmd::deny(run_id.clone(), req.id.clone(), d.reason, d.rule_id)
                            };

                            // 发送 decision
                            let _ = ctl_tx.send(serde_json::to_value(cmd).unwrap()).await;

                            // 本实现：decision 发出就从 pending 移除（更严格可等 ack）
                            pending.remove(&req.id);
                        }
                    }
                } else {
                    // line_rx closed：双通道都结束通常意味着子进程即将退出；这里不强行 abort
                }
            }

            // 超时检测
            _ = tick.tick() => {
                let now = Instant::now();
                let mut timed_out: Vec<String> = Vec::new();
                for (id, t0) in pending.iter() {
                    if now.duration_since(*t0) > decision_timeout {
                        timed_out.push(id.clone());
                    }
                }
                if !timed_out.is_empty() {
                    tracing::error!(error.kind="control.decision_timeout", ids=?timed_out);
                    if fail_closed {
                        abort_sequence(&mut child, &ctl_tx, &run_id, cfg.control.abort_grace_ms, "decision timeout").await;
                        return Ok(40);
                    }
                }
            }
        }
    }
}

async fn abort_sequence(
    child: &mut tokio::process::Child,
    ctl_tx: &mpsc::Sender<serde_json::Value>,
    run_id: &str,
    abort_grace_ms: u64,
    reason: &str,
) {
    // best-effort send policy.abort
    let abort = PolicyAbortCmd::new(run_id.to_string(), reason.to_string(), Some("policy_violation".into()));
    let _ = ctl_tx.send(serde_json::to_value(abort).unwrap()).await;

    // wait grace
    tokio::time::sleep(Duration::from_millis(abort_grace_ms)).await;

    // force kill (cross-platform)
    let _ = child.kill().await;
}

fn normalize_exit(status: std::process::ExitStatus) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(code) = status.code() { code }
        else if let Some(sig) = status.signal() { 128 + sig }
        else { 1 }
    }
    #[cfg(windows)]
    {
        status.code().unwrap_or(1)
    }
}