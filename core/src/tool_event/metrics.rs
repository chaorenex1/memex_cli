use crate::tool_event::ToolEvent;
use crate::tool_event::{correlate_request_result, CorrelationStats};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub struct ToolInsights {
    pub total: usize,
    pub by_type: BTreeMap<String, usize>,
    pub tools: Vec<String>,
    pub failing_tools: Vec<String>,
    pub last_request: Option<Value>,
    pub last_result: Option<Value>,
    pub correlation: CorrelationStats,
}

pub fn build_tool_insights(events: &[ToolEvent]) -> ToolInsights {
    let mut by_type: BTreeMap<String, usize> = BTreeMap::new();
    let mut tools: BTreeSet<String> = BTreeSet::new();
    let mut failing: BTreeSet<String> = BTreeSet::new();

    let mut last_req: Option<&ToolEvent> = None;
    let mut last_res: Option<&ToolEvent> = None;

    for e in events {
        *by_type.entry(e.event_type.clone()).or_insert(0) += 1;

        if let Some(t) = &e.tool {
            tools.insert(t.clone());
        }

        if e.event_type == "tool.request" {
            last_req = Some(e);
        } else if e.event_type == "tool.result" {
            last_res = Some(e);
            if e.ok == Some(false) {
                if let Some(t) = &e.tool {
                    failing.insert(t.clone());
                }
            }
        }
    }

    let correlation = correlate_request_result(events);

    ToolInsights {
        total: events.len(),
        by_type,
        tools: tools.into_iter().collect(),
        failing_tools: failing.into_iter().collect(),
        last_request: last_req.map(slim_event),
        last_result: last_res.map(slim_event),
        correlation,
    }
}

/// 将事件裁剪成“可回传摘要”，避免 payload 过大
fn slim_event(e: &ToolEvent) -> Value {
    serde_json::json!({
        "v": e.v,
        "type": e.event_type,
        "ts": e.ts,
        "id": e.id,
        "tool": e.tool,
        "action": e.action,
        "ok": e.ok,
        // args 可能很大：只保留 top-level keys
        "args_keys": args_keys(&e.args),
    })
}

fn args_keys(v: &Value) -> Vec<String> {
    match v.as_object() {
        Some(map) => map.keys().take(32).cloned().collect(),
        None => vec![],
    }
}
