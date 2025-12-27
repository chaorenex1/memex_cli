use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct PolicyDecisionCmd {
    pub v: u8,
    #[serde(rename = "type")]
    pub ty: &'static str,
    pub ts: String,
    pub run_id: String,
    pub id: String,
    pub decision: &'static str,
    pub reason: String,
    pub rule_id: Option<String>,
}

impl PolicyDecisionCmd {
    pub fn allow(run_id: String, id: String, reason: String, rule_id: Option<String>) -> Self {
        Self {
            v: 1,
            ty: "policy.decision",
            ts: chrono::Utc::now().to_rfc3339(),
            run_id,
            id,
            decision: "allow",
            reason,
            rule_id,
        }
    }

    pub fn deny(run_id: String, id: String, reason: String, rule_id: Option<String>) -> Self {
        Self {
            v: 1,
            ty: "policy.decision",
            ts: chrono::Utc::now().to_rfc3339(),
            run_id,
            id,
            decision: "deny",
            reason,
            rule_id,
        }
    }
}


#[derive(Debug, Serialize)]
pub struct PolicyAbortCmd {
    pub v: u8,
    #[serde(rename = "type")]
    pub ty: &'static str, // "policy.abort"
    pub ts: String,
    pub run_id: String,
    pub id: String,       // e.g. "abort-1"
    pub reason: String,
    pub code: Option<String>,
}

impl PolicyAbortCmd {
    pub fn new(run_id: String, reason: String, code: Option<String>) -> Self {
        Self {
            v: 1,
            ty: "policy.abort",
            ts: chrono::Utc::now().to_rfc3339(),
            run_id,
            id: "abort-1".into(),
            reason,
            code,
        }
    }
}

