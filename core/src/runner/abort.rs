use serde::Serialize;
use tokio::sync::mpsc;

use super::traits::RunnerSession;
use super::types::Signal;

pub async fn abort_sequence(
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
    tokio::time::sleep(std::time::Duration::from_millis(abort_grace_ms)).await;
    let _ = session.signal(Signal::Kill).await;
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
    fn new(run_id: String, reason: String, code: Option<String>) -> Self {
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
