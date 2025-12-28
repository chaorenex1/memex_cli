use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

use crate::tool_event::ToolEvent;

#[derive(Debug, Clone, Default, Serialize)]
pub struct CorrelationStats {
    pub request_count: usize,
    pub result_count: usize,
    pub matched_pairs: usize,
    pub unmatched_requests: usize,
    pub unmatched_results: usize,
    pub request_missing_id: usize,
    pub result_missing_id: usize,
    pub duplicate_request_ids: usize,
    pub duplicate_result_ids: usize,
    pub failed_results: usize,
    pub by_tool: BTreeMap<String, ToolCorrStats>,
    pub last_pair: Option<Value>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ToolCorrStats {
    pub matched: usize,
    pub failed: usize,
    pub request_only: usize,
    pub result_only: usize,
    pub request_missing_id: usize,
    pub result_missing_id: usize,
}

pub fn correlate_request_result(events: &[ToolEvent]) -> CorrelationStats {
    let mut stats = CorrelationStats::default();

    let mut req_by_id: BTreeMap<String, &ToolEvent> = BTreeMap::new();
    let mut res_by_id: BTreeMap<String, &ToolEvent> = BTreeMap::new();

    let mut seen_req_ids: BTreeSet<String> = BTreeSet::new();
    let mut seen_res_ids: BTreeSet<String> = BTreeSet::new();

    for e in events {
        match e.event_type.as_str() {
            "tool.request" => {
                stats.request_count += 1;
                let tool = tool_name(e);
                let entry = stats.by_tool.entry(tool).or_default();

                match e.id.as_deref() {
                    Some(id) if !id.trim().is_empty() => {
                        if !seen_req_ids.insert(id.to_string()) {
                            stats.duplicate_request_ids += 1;
                        }
                        req_by_id.insert(id.to_string(), e);
                    }
                    _ => {
                        stats.request_missing_id += 1;
                        entry.request_missing_id += 1;
                    }
                }
            }
            "tool.result" => {
                stats.result_count += 1;
                let tool = tool_name(e);
                let entry = stats.by_tool.entry(tool).or_default();

                if e.ok == Some(false) {
                    stats.failed_results += 1;
                }

                match e.id.as_deref() {
                    Some(id) if !id.trim().is_empty() => {
                        if !seen_res_ids.insert(id.to_string()) {
                            stats.duplicate_result_ids += 1;
                        }
                        res_by_id.insert(id.to_string(), e);
                    }
                    _ => {
                        stats.result_missing_id += 1;
                        entry.result_missing_id += 1;
                    }
                }
            }
            _ => {}
        }
    }

    let mut matched = 0usize;

    for (id, req) in req_by_id.iter() {
        if let Some(res) = res_by_id.get(id) {
            matched += 1;

            let tool = tool_name(req);
            let entry = stats.by_tool.entry(tool).or_default();
            entry.matched += 1;
            if res.ok == Some(false) {
                entry.failed += 1;
            }

            stats.last_pair = Some(slim_pair(id, req, res));
        } else {
            stats.unmatched_requests += 1;
            let tool = tool_name(req);
            let entry = stats.by_tool.entry(tool).or_default();
            entry.request_only += 1;
        }
    }

    for (id, res) in res_by_id.iter() {
        if !req_by_id.contains_key(id) {
            stats.unmatched_results += 1;
            let tool = tool_name(res);
            let entry = stats.by_tool.entry(tool).or_default();
            entry.result_only += 1;
        }
    }

    stats.matched_pairs = matched;
    stats
}

fn tool_name(e: &ToolEvent) -> String {
    e.tool.clone().unwrap_or_else(|| "unknown".to_string())
}

fn slim_pair(id: &str, req: &ToolEvent, res: &ToolEvent) -> Value {
    serde_json::json!({
        "id": id,
        "tool": req.tool,
        "action": req.action,
        "req_ts": req.ts,
        "res_ts": res.ts,
        "ok": res.ok,
        "req_args_keys": args_keys(&req.args),
        "res_output_keys": res.output.as_ref().and_then(|v| v.as_object().map(|o| o.keys().take(32).cloned().collect::<Vec<_>>())),
    })
}

fn args_keys(v: &Value) -> Vec<String> {
    match v.as_object() {
        Some(map) => map.keys().take(32).cloned().collect(),
        None => vec![],
    }
}
