use std::collections::BTreeMap;

use crate::tool_event::ToolEvent;
use crate::tool_event::WrapperEvent;
use crate::tool_event::{MultiToolEventLineParser, TOOL_EVENT_PREFIX};

use super::model::ReplayRun;

pub fn parse_events_file(path: &str, run_id: Option<&str>) -> Result<Vec<ReplayRun>, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut runs: BTreeMap<String, ReplayRun> = BTreeMap::new();
    let mut run_order: Vec<String> = Vec::new();
    let mut current_run_id: Option<String> = None;

    let mut parser = MultiToolEventLineParser::new(TOOL_EVENT_PREFIX);

    for line in raw.lines() {
        let s = line.trim();
        if s.is_empty() {
            continue;
        }

        if let Some(ev) = parser.parse_line(s) {
            if let Some(id) = current_run_id.clone() {
                if run_id.map(|r| r == id).unwrap_or(true) {
                    attach_tool_event(&mut runs, &mut run_order, id, ev);
                }
            }
            continue;
        }

        if let Ok(w) = serde_json::from_str::<WrapperEvent>(s) {
            if let Some(id) = w.run_id.clone() {
                current_run_id = Some(id.clone());
                if run_id.map(|r| r == id).unwrap_or(true) {
                    attach_wrapper(&mut runs, &mut run_order, id, w);
                }
            }
        }
    }

    let mut out = Vec::new();
    for id in run_order {
        if let Some(run) = runs.remove(&id) {
            out.push(run);
        }
    }
    Ok(out)
}

fn attach_tool_event(
    runs: &mut BTreeMap<String, ReplayRun>,
    run_order: &mut Vec<String>,
    run_id: String,
    ev: ToolEvent,
) {
    let run = runs.entry(run_id.clone()).or_insert_with(|| {
        run_order.push(run_id.clone());
        ReplayRun {
            run_id,
            ..Default::default()
        }
    });
    run.tool_events.push(ev);
}

fn attach_wrapper(
    runs: &mut BTreeMap<String, ReplayRun>,
    run_order: &mut Vec<String>,
    run_id: String,
    w: WrapperEvent,
) {
    let run = runs.entry(run_id.clone()).or_insert_with(|| {
        run_order.push(run_id.clone());
        ReplayRun {
            run_id,
            ..Default::default()
        }
    });

    match w.event_type.as_str() {
        "runner.start" => run.runner_start = Some(w),
        "runner.exit" => run.runner_exit = Some(w),
        "tee.drop" => run.tee_drop = Some(w),
        "memory.search.result" => run.search_result = Some(w),
        "gatekeeper.decision" => run.gatekeeper_decision = Some(w),
        "memory.call" => run.memory_calls.push(w),
        _ => run.memory_calls.push(w),
    }
}
