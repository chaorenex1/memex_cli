use crate::gatekeeper::{Gatekeeper, GatekeeperConfig, SearchMatch};
use crate::memory::parse_search_matches;
use crate::replay::model::ReplayRun;
use crate::runner::RunOutcome;

pub struct GatekeeperReplayResult {
    pub skipped: bool,
    pub skip_reason: Option<String>,
    pub decision_json: serde_json::Value,
}

pub fn rerun_gatekeeper_for_run(
    run: &ReplayRun,
    gk_cfg: &GatekeeperConfig,
) -> GatekeeperReplayResult {
    let Some(sr) = &run.search_result else {
        return GatekeeperReplayResult {
            skipped: true,
            skip_reason: Some("missing memory.search.result in events".to_string()),
            decision_json: serde_json::Value::Null,
        };
    };
    let Some(data) = &sr.data else {
        return GatekeeperReplayResult {
            skipped: true,
            skip_reason: Some("memory.search.result missing data".to_string()),
            decision_json: serde_json::Value::Null,
        };
    };

    let matches_v = data
        .get("matches")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let matches: Vec<SearchMatch> = match parse_search_matches(&matches_v) {
        Ok(m) => m,
        Err(e) => {
            return GatekeeperReplayResult {
                skipped: true,
                skip_reason: Some(format!("failed to parse search matches: {}", e)),
                decision_json: serde_json::Value::Null,
            }
        }
    };

    let outcome = build_run_outcome_from_exit(run);

    let now = chrono::Utc::now();
    let decision = Gatekeeper::evaluate(gk_cfg, now, &matches, &outcome, &run.tool_events);

    GatekeeperReplayResult {
        skipped: false,
        skip_reason: None,
        decision_json: serde_json::to_value(&decision).unwrap_or(serde_json::Value::Null),
    }
}

fn build_run_outcome_from_exit(run: &ReplayRun) -> RunOutcome {
    let mut out = RunOutcome {
        exit_code: -999,
        duration_ms: None,
        stdout_tail: String::new(),
        stderr_tail: String::new(),
        tool_events: run.tool_events.clone(),
        shown_qa_ids: vec![],
        used_qa_ids: vec![],
    };

    if let Some(exit) = &run.runner_exit {
        if let Some(d) = &exit.data {
            out.exit_code = d.get("exit_code").and_then(|v| v.as_i64()).unwrap_or(-999) as i32;
            out.duration_ms = d
                .get("duration_ms")
                .and_then(|v| v.as_i64())
                .map(|x| x as u64);
            out.stdout_tail = d
                .get("stdout_tail")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            out.stderr_tail = d
                .get("stderr_tail")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            out.shown_qa_ids = d
                .get("shown_qa_ids")
                .and_then(|v| v.as_array())
                .map(|a| arr_str(a))
                .unwrap_or_default();
            out.used_qa_ids = d
                .get("used_qa_ids")
                .and_then(|v| v.as_array())
                .map(|a| arr_str(a))
                .unwrap_or_default();
        }
    }

    out
}

fn arr_str(a: &[serde_json::Value]) -> Vec<String> {
    a.iter()
        .filter_map(|x| x.as_str().map(|s| s.to_string()))
        .collect()
}
