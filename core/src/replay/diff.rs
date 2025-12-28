use serde_json::Value;

#[derive(Debug, Clone)]
pub struct DecisionDiff {
    pub has_baseline: bool,
    pub changed: bool,
    pub summary_lines: Vec<String>,
}

pub fn diff_gatekeeper_decision(baseline: Option<&Value>, rerun: &Value) -> DecisionDiff {
    let mut lines = Vec::new();

    let (b_inject, b_candidate, b_signals) = if let Some(b) = baseline {
        (
            get_inject_ids(b),
            get_bool(b, "should_write_candidate"),
            b.get("signals").cloned(),
        )
    } else {
        (vec![], None, None)
    };

    let r_inject = get_inject_ids(rerun);
    let r_candidate = get_bool(rerun, "should_write_candidate");
    let r_signals = rerun.get("signals").cloned();

    if baseline.is_some() {
        if b_inject != r_inject {
            lines.push(format!(
                "inject_list changed: baseline={:?} rerun={:?}",
                b_inject, r_inject
            ));
        }
        if b_candidate != r_candidate {
            lines.push(format!(
                "should_write_candidate changed: baseline={:?} rerun={:?}",
                b_candidate, r_candidate
            ));
        }
        let keys = [
            "tool_events_total",
            "has_strong",
            "top1_score",
            "status_reject",
            "stale_reject",
            "fail_reject",
        ];
        for k in keys {
            let bv = b_signals.as_ref().and_then(|x| x.get(k)).cloned();
            let rv = r_signals.as_ref().and_then(|x| x.get(k)).cloned();
            if bv != rv {
                lines.push(format!(
                    "signals.{k} changed: baseline={:?} rerun={:?}",
                    bv, rv
                ));
            }
        }
    } else {
        lines.push(format!("rerun inject_list: {:?}", r_inject));
        lines.push(format!("rerun should_write_candidate: {:?}", r_candidate));
    }

    let changed = baseline.is_some() && !lines.is_empty();
    DecisionDiff {
        has_baseline: baseline.is_some(),
        changed,
        summary_lines: lines,
    }
}

fn get_inject_ids(v: &Value) -> Vec<String> {
    let mut ids = Vec::new();
    if let Some(arr) = v.get("inject_list").and_then(|x| x.as_array()) {
        for it in arr {
            if let Some(id) = it.get("qa_id").and_then(|x| x.as_str()) {
                ids.push(id.to_string());
            }
        }
    }
    ids
}

fn get_bool(v: &Value, k: &str) -> Option<bool> {
    v.get(k).and_then(|x| x.as_bool())
}
