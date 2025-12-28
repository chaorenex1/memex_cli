use regex::Regex;
use serde_json::Value;

use crate::gatekeeper::SearchMatch;
use crate::runner::RunOutcome;
use crate::tool_event::CorrelationStats;

#[derive(Debug, Clone)]
pub struct ValidationSignal {
    pub result: String,
    pub signal_strength: String,
    pub strong_signal: bool,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct SignalHeuristics {
    pub success_patterns: Vec<Regex>,
    pub fail_patterns: Vec<Regex>,
}

impl Default for SignalHeuristics {
    fn default() -> Self {
        let success = vec![
            Regex::new(r"(?i)\btests?\s+passed\b").unwrap(),
            Regex::new(r"(?i)\ball\s+tests?\s+passed\b").unwrap(),
            Regex::new(r"(?i)\bbuild\s+succeeded\b").unwrap(),
            Regex::new(r"(?i)\bcompile(d)?\s+success(fully)?\b").unwrap(),
            Regex::new(r"(?i)\bfinished\b.*\bsuccess\b").unwrap(),
            Regex::new(r"(?i)\bpass(ed)?\b").unwrap(),
            Regex::new(r"(?i)\bok\b").unwrap(),
        ];

        let fail = vec![
            Regex::new(r"(?i)\bfailed\b").unwrap(),
            Regex::new(r"(?i)\berror\b").unwrap(),
            Regex::new(r"(?i)\bpanic\b").unwrap(),
            Regex::new(r"(?i)\bexception\b").unwrap(),
            Regex::new(r"(?i)\btraceback\b").unwrap(),
        ];

        Self {
            success_patterns: success,
            fail_patterns: fail,
        }
    }
}

pub fn grade_validation_signal(
    exit_code: i32,
    stdout_tail: &str,
    stderr_tail: &str,
    used_qa_ids_count: usize,
    heur: &SignalHeuristics,
    failing_tools_count: usize,
) -> ValidationSignal {
    let joined = format!("{stdout_tail}\n{stderr_tail}");

    let is_pass = exit_code == 0;
    let hit_success = heur.success_patterns.iter().any(|re| re.is_match(&joined));
    let hit_fail = heur.fail_patterns.iter().any(|re| re.is_match(&joined));

    let result = if is_pass { "pass" } else { "fail" }.to_string();

    let (signal_strength, strong_signal, reason) =
        if is_pass && hit_success && used_qa_ids_count > 0 && failing_tools_count == 0 {
            (
                "strong".to_string(),
                true,
                "exit_code=0 + success markers + QA used".to_string(),
            )
        } else if is_pass && (hit_success || used_qa_ids_count > 0) {
            (
                "medium".to_string(),
                false,
                "exit_code=0 but not strong-enough markers".to_string(),
            )
        } else if !is_pass && hit_fail {
            (
                "medium".to_string(),
                false,
                "exit_code!=0 with explicit failure markers".to_string(),
            )
        } else {
            (
                "weak".to_string(),
                false,
                "insufficient evidence for strong/medium".to_string(),
            )
        };

    ValidationSignal {
        result,
        signal_strength,
        strong_signal,
        reason,
    }
}

pub fn build_signals(
    matches: &[SearchMatch],
    run: &RunOutcome,
    tool_corr: &CorrelationStats,
) -> Value {
    let mut by_type: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut tools: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut failing: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

    for e in &run.tool_events {
        *by_type.entry(e.event_type.clone()).or_insert(0) += 1;
        if let Some(t) = &e.tool {
            tools.insert(t.clone());
        }
        if e.event_type == "tool.result" && e.ok == Some(false) {
            if let Some(t) = &e.tool {
                failing.insert(t.clone());
            }
        }
    }

    serde_json::json!({
        "matches_total": matches.len(),
        "tool_events_total": run.tool_events.len(),
        "tool_events_by_type": by_type,
        "tools": tools.into_iter().collect::<Vec<_>>(),
        "failing_tools": failing.into_iter().collect::<Vec<_>>(),
        "tool_corr": {
            "request_count": tool_corr.request_count,
            "result_count": tool_corr.result_count,
            "matched_pairs": tool_corr.matched_pairs,
            "unmatched_requests": tool_corr.unmatched_requests,
            "unmatched_results": tool_corr.unmatched_results,
            "request_missing_id": tool_corr.request_missing_id,
            "result_missing_id": tool_corr.result_missing_id,
            "duplicate_request_ids": tool_corr.duplicate_request_ids,
            "duplicate_result_ids": tool_corr.duplicate_result_ids,
            "failed_results": tool_corr.failed_results,
            "by_tool": tool_corr.by_tool,
            "last_pair": tool_corr.last_pair,
        },
    })
}
