use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::mpsc;

use crate::tool_event::ToolEvent;

use super::traits::PolicyPlugin;
use super::types::PolicyAction;

#[derive(Debug, Clone, Copy)]
pub enum PolicyDecision {
    Allow,
    Deny,
}

#[derive(Debug)]
pub enum PolicyOutcome {
    Continue,
    Abort(String),
}

#[derive(Debug)]
struct PendingDecision {
    started_at: Instant,
    prompt: String,
}

pub struct PolicyEngine {
    fail_closed: bool,
    decision_timeout: Duration,
    decided_ids: HashSet<String>,
    pending: HashMap<String, PendingDecision>,
}

impl PolicyEngine {
    pub fn new(fail_closed: bool, decision_timeout: Duration) -> Self {
        Self {
            fail_closed,
            decision_timeout,
            decided_ids: HashSet::new(),
            pending: HashMap::new(),
        }
    }

    pub async fn on_tool_request(
        &mut self,
        ev: &ToolEvent,
        policy: Option<&dyn PolicyPlugin>,
        ctl_tx: &mpsc::Sender<serde_json::Value>,
        run_id: &str,
    ) -> PolicyOutcome {
        let Some(id) = ev.id.as_deref().map(str::to_string) else {
            return if self.fail_closed {
                PolicyOutcome::Abort("tool.request missing id".to_string())
            } else {
                PolicyOutcome::Continue
            };
        };

        if self.decided_ids.contains(&id) {
            return PolicyOutcome::Continue;
        }

        let action = match policy {
            Some(p) => p.check(ev).await,
            None => PolicyAction::Allow,
        };

        match action {
            PolicyAction::Allow => {
                if let Err(e) =
                    send_policy_decision(ctl_tx, run_id, &id, PolicyDecision::Allow, "allowed")
                        .await
                {
                    if self.fail_closed {
                        return PolicyOutcome::Abort(format!("policy.decision write failed: {e}"));
                    }
                }
                self.decided_ids.insert(id);
                PolicyOutcome::Continue
            }
            PolicyAction::Deny { reason } => {
                let _ = send_policy_decision(ctl_tx, run_id, &id, PolicyDecision::Deny, &reason).await;
                self.decided_ids.insert(id);
                PolicyOutcome::Abort(format!("policy denial: {reason}"))
            }
            PolicyAction::Ask { prompt } => {
                let reason = format!("policy requires approval: {prompt}");
                let _ = send_policy_decision(ctl_tx, run_id, &id, PolicyDecision::Deny, &reason)
                    .await;
                self.decided_ids.insert(id);
                PolicyOutcome::Abort(reason)
            }
        }
    }

    pub async fn on_tick(
        &mut self,
        now: Instant,
        ctl_tx: &mpsc::Sender<serde_json::Value>,
        run_id: &str,
    ) -> PolicyOutcome {
        if self.pending.is_empty() {
            return PolicyOutcome::Continue;
        }

        let mut timed_out_ids: Vec<String> = vec![];
        for (id, p) in &self.pending {
            if now.duration_since(p.started_at) > self.decision_timeout {
                timed_out_ids.push(id.clone());
            }
        }

        if timed_out_ids.is_empty() {
            return PolicyOutcome::Continue;
        }

        for id in timed_out_ids {
            let prompt = self
                .pending
                .remove(&id)
                .map(|p| p.prompt)
                .unwrap_or_else(|| "policy approval required".to_string());
            let reason = format!("policy decision timeout: {prompt}");
            let _ = send_policy_decision(ctl_tx, run_id, &id, PolicyDecision::Deny, &reason).await;
            self.decided_ids.insert(id);
        }

        if self.fail_closed {
            PolicyOutcome::Abort("decision timeout".to_string())
        } else {
            PolicyOutcome::Continue
        }
    }
}

#[derive(Debug, Serialize)]
struct PolicyDecisionCmd<'a> {
    pub v: u8,
    #[serde(rename = "type")]
    pub ty: &'static str,
    pub ts: String,
    pub run_id: &'a str,
    pub id: &'a str,
    pub decision: &'static str,
    pub reason: &'a str,
}

async fn send_policy_decision(
    ctl_tx: &mpsc::Sender<serde_json::Value>,
    run_id: &str,
    id: &str,
    decision: PolicyDecision,
    reason: &str,
) -> Result<(), mpsc::error::SendError<serde_json::Value>> {
    let decision_str = match decision {
        PolicyDecision::Allow => "allow",
        PolicyDecision::Deny => "deny",
    };
    let cmd = PolicyDecisionCmd {
        v: 1,
        ty: "policy.decision",
        ts: chrono::Utc::now().to_rfc3339(),
        run_id,
        id,
        decision: decision_str,
        reason,
    };
    ctl_tx.send(serde_json::to_value(cmd).unwrap()).await
}
