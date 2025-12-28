use std::cmp::Reverse;

use crate::tool_event::{CorrelationStats, ToolCorrStats};

pub fn summarize_tool_corr_anomalies(corr: &CorrelationStats) -> Vec<String> {
    let mut reasons = Vec::new();

    reasons.push(format!(
        "tool_corr: req={}, res={}, matched={}, unreq={}, unres={}, miss_req_id={}, miss_res_id={}, dup_req_id={}, dup_res_id={}, failed_res={}",
        corr.request_count,
        corr.result_count,
        corr.matched_pairs,
        corr.unmatched_requests,
        corr.unmatched_results,
        corr.request_missing_id,
        corr.result_missing_id,
        corr.duplicate_request_ids,
        corr.duplicate_result_ids,
        corr.failed_results,
    ));

    if corr.request_missing_id + corr.result_missing_id > 0 {
        reasons.push(format!(
            "tool_corr anomaly: missing id (request={}, result={})",
            corr.request_missing_id, corr.result_missing_id
        ));
        reasons.extend(top_tools_lines(&corr.by_tool, Kind::MissingId, 5));
    }

    if corr.unmatched_requests + corr.unmatched_results > 0 {
        reasons.push(format!(
            "tool_corr anomaly: unmatched (requests_only={}, results_only={})",
            corr.unmatched_requests, corr.unmatched_results
        ));
        reasons.extend(top_tools_lines(&corr.by_tool, Kind::Unmatched, 5));
    }

    if corr.duplicate_request_ids + corr.duplicate_result_ids > 0 {
        reasons.push(format!(
            "tool_corr anomaly: duplicate ids (req_dup={}, res_dup={})",
            corr.duplicate_request_ids, corr.duplicate_result_ids
        ));
    }

    if corr.failed_results > 0 {
        reasons.push(format!("tool_corr: failed_results={}", corr.failed_results));
        reasons.extend(top_tools_lines(&corr.by_tool, Kind::Failed, 5));
    }

    if corr.last_pair.is_some() {
        reasons.push("tool_corr: last_pair available".to_string());
    }

    reasons
}

#[derive(Clone, Copy)]
enum Kind {
    MissingId,
    Unmatched,
    Failed,
}

fn top_tools_lines(
    by_tool: &std::collections::BTreeMap<String, ToolCorrStats>,
    kind: Kind,
    top_n: usize,
) -> Vec<String> {
    let mut rows: Vec<(String, usize, ToolCorrStats)> = Vec::new();

    for (tool, s) in by_tool.iter() {
        let score = match kind {
            Kind::MissingId => s.request_missing_id + s.result_missing_id,
            Kind::Unmatched => s.request_only + s.result_only,
            Kind::Failed => s.failed,
        };
        if score > 0 {
            rows.push((tool.clone(), score, s.clone()));
        }
    }

    rows.sort_by_key(|(_, score, _)| Reverse(*score));

    rows.into_iter()
        .take(top_n)
        .map(|(tool, score, s)| match kind {
            Kind::MissingId => format!(
                " - tool={} missing_id={} (req_missing={}, res_missing={})",
                tool, score, s.request_missing_id, s.result_missing_id
            ),
            Kind::Unmatched => format!(
                " - tool={} unmatched={} (request_only={}, result_only={})",
                tool, score, s.request_only, s.result_only
            ),
            Kind::Failed => format!(
                " - tool={} failed={} (matched={}, request_only={}, result_only={})",
                tool, score, s.matched, s.request_only, s.result_only
            ),
        })
        .collect()
}
