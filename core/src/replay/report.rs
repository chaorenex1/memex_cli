use serde_json::Value;

use super::model::ReplayRun;

pub fn build_report(runs: &[ReplayRun]) -> Value {
    let mut total_tool_events = 0usize;
    let mut runs_with_exit = 0usize;
    let mut runs_with_drop = 0usize;
    let mut runs_with_search = 0usize;

    let mut run_items = Vec::new();

    for r in runs {
        let tool_count = r.tool_events.len();
        total_tool_events += tool_count;
        if r.runner_exit.is_some() {
            runs_with_exit += 1;
        }
        if r.tee_drop.is_some() {
            runs_with_drop += 1;
        }
        if r.search_result.is_some() {
            runs_with_search += 1;
        }

        run_items.push(serde_json::json!({
            "run_id": r.run_id,
            "tool_events": tool_count,
            "has_exit": r.runner_exit.is_some(),
            "has_drop": r.tee_drop.is_some(),
            "has_search": r.search_result.is_some(),
            "derived": r.derived,
        }));
    }

    serde_json::json!({
        "totals": {
            "runs": runs.len(),
            "tool_events": total_tool_events,
            "runs_with_exit": runs_with_exit,
            "runs_with_drop": runs_with_drop,
            "runs_with_search": runs_with_search,
        },
        "runs": run_items,
    })
}

pub fn format_text(report: &Value) -> String {
    let mut out = String::new();
    let totals = report.get("totals");

    out.push_str(
        "Replay report
",
    );
    if let Some(t) = totals {
        out.push_str(&format!(
            "runs: {}
",
            t.get("runs").unwrap_or(&Value::Null)
        ));
        out.push_str(&format!(
            "tool_events: {}
",
            t.get("tool_events").unwrap_or(&Value::Null)
        ));
        out.push_str(&format!(
            "runs_with_exit: {}
",
            t.get("runs_with_exit").unwrap_or(&Value::Null)
        ));
        out.push_str(&format!(
            "runs_with_drop: {}
",
            t.get("runs_with_drop").unwrap_or(&Value::Null)
        ));
        out.push_str(&format!(
            "runs_with_search: {}
",
            t.get("runs_with_search").unwrap_or(&Value::Null)
        ));
    }

    if let Some(runs) = report.get("runs").and_then(|v| v.as_array()) {
        for r in runs {
            out.push_str(&format!(
                "- run_id: {}
",
                r.get("run_id").unwrap_or(&Value::Null)
            ));
            out.push_str(&format!(
                "  tool_events: {}
",
                r.get("tool_events").unwrap_or(&Value::Null)
            ));
            out.push_str(&format!(
                "  has_exit: {}
",
                r.get("has_exit").unwrap_or(&Value::Null)
            ));
            out.push_str(&format!(
                "  has_drop: {}
",
                r.get("has_drop").unwrap_or(&Value::Null)
            ));
            out.push_str(&format!(
                "  has_search: {}
",
                r.get("has_search").unwrap_or(&Value::Null)
            ));

            if let Some(derived) = r.get("derived") {
                if let Some(rerun) = derived.get("rerun_gatekeeper") {
                    let skipped = rerun.get("skipped").unwrap_or(&Value::Null);
                    let changed = rerun
                        .get("diff")
                        .and_then(|d| d.get("changed"))
                        .unwrap_or(&Value::Null);
                    let reason = rerun.get("skip_reason").unwrap_or(&Value::Null);
                    out.push_str(&format!(
                        "  rerun_gatekeeper: skipped={} changed={} reason={}
",
                        skipped, changed, reason
                    ));

                    if let Some(lines) = rerun
                        .get("diff")
                        .and_then(|d| d.get("summary_lines"))
                        .and_then(|v| v.as_array())
                    {
                        let mut items = Vec::new();
                        for it in lines {
                            if let Some(s) = it.as_str() {
                                items.push(s.to_string());
                            }
                        }
                        if !items.is_empty() {
                            out.push_str(&format!(
                                "  rerun_diff: {}
",
                                items.join(" | ")
                            ));
                        }
                    }
                }
            }
        }
    }

    out
}
