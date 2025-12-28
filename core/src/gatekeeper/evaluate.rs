use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashSet;

use super::gatekeeper_reasons::summarize_tool_corr_anomalies;
use crate::runner::RunOutcome;
use crate::tool_event::{build_tool_insights, ToolEvent};

use super::config::GatekeeperConfig;
use super::decision::{GatekeeperDecision, HitRef, InjectItem, SearchMatch, ValidatePlan};
use super::signals::{build_signals, grade_validation_signal, SignalHeuristics};

pub struct Gatekeeper;

impl Gatekeeper {
    pub fn evaluate(
        cfg: &GatekeeperConfig,
        now: DateTime<Utc>,
        matches: &[SearchMatch],
        run: &RunOutcome,
        tool_events: &[ToolEvent],
    ) -> GatekeeperDecision {
        let mut reasons: Vec<String> = Vec::new();

        let top1_score = matches
            .iter()
            .map(|m| m.score)
            .fold(None, |acc, x| Some(acc.map_or(x, |a: f32| a.max(x))));
        if let Some(s) = top1_score {
            reasons.push(format!("top1_score={:.3}", s));
        }

        let mut usable: Vec<&SearchMatch> = Vec::new();
        let mut stale_count = 0usize;
        let mut status_reject = 0usize;
        let mut fail_reject = 0usize;

        for m in matches.iter() {
            if !cfg.active_statuses.contains(&m.status) {
                status_reject += 1;
                continue;
            }

            if cfg.exclude_stale_by_default && is_stale(m, now) {
                stale_count += 1;
                continue;
            }

            let cf = extract_i32(&m.metadata, "consecutive_fail").unwrap_or(0);
            if cf >= cfg.block_if_consecutive_fail_ge {
                fail_reject += 1;
                continue;
            }

            usable.push(m);
        }

        reasons.push(format!(
            "filtered: usable={}, status_reject={}, stale_reject={}, fail_reject={}",
            usable.len(),
            status_reject,
            stale_count,
            fail_reject
        ));

        usable.sort_by(|a, b| {
            let key_a = (a.validation_level, a.trust, a.score, a.freshness);
            let key_b = (b.validation_level, b.trust, b.score, b.freshness);

            key_b
                .0
                .cmp(&key_a.0)
                .then_with(|| {
                    key_b
                        .1
                        .partial_cmp(&key_a.1)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| {
                    key_b
                        .2
                        .partial_cmp(&key_a.2)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| {
                    key_b
                        .3
                        .partial_cmp(&key_a.3)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });

        let has_strong = usable
            .iter()
            .any(|m| m.validation_level >= cfg.min_level_inject);

        let mut inject_list: Vec<InjectItem> = Vec::new();

        for m in usable.iter() {
            if inject_list.len() >= cfg.max_inject {
                break;
            }
            if m.validation_level >= cfg.min_level_inject && m.trust >= cfg.min_trust_show {
                inject_list.push(to_inject_item(m));
            }
        }

        if inject_list.is_empty() && !usable.is_empty() && !has_strong {
            for m in usable.iter().take(cfg.max_inject) {
                if m.validation_level >= cfg.min_level_fallback && m.trust >= cfg.min_trust_show {
                    reasons.push("inject fallback (no strong matches)".to_string());
                    inject_list.push(to_inject_item(m));
                    break;
                }
            }
        }

        reasons.push(format!(
            "inject: count={}, has_strong={}",
            inject_list.len(),
            has_strong
        ));

        let mut should_write_candidate = true;

        if has_strong {
            should_write_candidate = false;
            reasons.push("candidate suppressed: has strong matches".into());
        }

        if let Some(s) = top1_score {
            if s >= cfg.skip_if_top1_score_ge {
                should_write_candidate = false;
                reasons.push(format!(
                    "candidate suppressed: top1_score >= {:.2}",
                    cfg.skip_if_top1_score_ge
                ));
            }
        }

        let shown: HashSet<String> = run.shown_qa_ids.iter().cloned().collect();
        let used: HashSet<String> = run.used_qa_ids.iter().cloned().collect();

        let mut hit_refs: Vec<HitRef> = Vec::new();
        for qa_id in shown.union(&used) {
            hit_refs.push(HitRef {
                qa_id: qa_id.clone(),
                shown: shown.contains(qa_id),
                used: used.contains(qa_id),
                message_id: None,
                context: None,
            });
        }

        let insights = build_tool_insights(tool_events);
        let corr = &insights.correlation;

        let heur = SignalHeuristics::default();
        let sig = grade_validation_signal(
            run.exit_code,
            &run.stdout_tail,
            &run.stderr_tail,
            run.used_qa_ids.len(),
            &heur,
            insights.failing_tools.len(),
        );

        let mut validate_targets: Vec<String> = Vec::new();
        if !run.used_qa_ids.is_empty() {
            validate_targets.extend(run.used_qa_ids.iter().cloned());
        } else if let Some(first) = inject_list.first() {
            validate_targets.push(first.qa_id.clone());
        }

        let mut validate_plans = Vec::new();
        for qa_id in validate_targets {
            validate_plans.push(ValidatePlan {
                qa_id,
                result: sig.result.clone(),
                signal_strength: sig.signal_strength.clone(),
                strong_signal: sig.strong_signal,
                context: Some(format!(
                    "exit_code={}, duration_ms={:?}, reason={}",
                    run.exit_code, run.duration_ms, sig.reason
                )),
                payload: serde_json::json!({
                    "exit_code": run.exit_code,
                    "duration_ms": run.duration_ms,
                    "stdout_tail_digest": digest_cheap(
                        &run.stdout_tail,
                        cfg.digest_head_chars,
                        cfg.digest_tail_chars,
                    ),
                    "stderr_tail_digest": digest_cheap(
                        &run.stderr_tail,
                        cfg.digest_head_chars,
                        cfg.digest_tail_chars,
                    ),
                    "tool_events_total": insights.total,
                    "tool_events_by_type": insights.by_type,
                    "tools": insights.tools,
                    "failing_tools": insights.failing_tools,
                    "last_tool_request": insights.last_request,
                    "last_tool_result": insights.last_result,
                    "tool_corr": {
                        "request_count": corr.request_count,
                        "result_count": corr.result_count,
                        "matched_pairs": corr.matched_pairs,
                        "unmatched_requests": corr.unmatched_requests,
                        "unmatched_results": corr.unmatched_results,
                        "missing_id": {
                            "request": corr.request_missing_id,
                            "result": corr.result_missing_id
                        },
                        "duplicates": {
                            "request_ids": corr.duplicate_request_ids,
                            "result_ids": corr.duplicate_result_ids
                        },
                        "failed_results": corr.failed_results
                    },
                    "last_pair": corr.last_pair,
                }),
            });
        }

        reasons.extend(summarize_tool_corr_anomalies(corr));

        let mut signals = build_signals(matches, run, corr);
        if let Some(map) = signals.as_object_mut() {
            map.insert("usable_count".into(), serde_json::json!(usable.len()));
            map.insert("inject_count".into(), serde_json::json!(inject_list.len()));
            map.insert("has_strong".into(), serde_json::json!(has_strong));
            map.insert("top1_score".into(), serde_json::json!(top1_score));
            map.insert("status_reject".into(), serde_json::json!(status_reject));
            map.insert("stale_reject".into(), serde_json::json!(stale_count));
            map.insert("fail_reject".into(), serde_json::json!(fail_reject));
            map.insert(
                "should_write_candidate".into(),
                serde_json::json!(should_write_candidate),
            );
            map.insert(
                "tool_events_total".into(),
                serde_json::json!(insights.total),
            );
            map.insert(
                "tool_events_by_type".into(),
                serde_json::json!(insights.by_type),
            );
            map.insert("tools".into(), serde_json::json!(insights.tools));
            map.insert(
                "failing_tools".into(),
                serde_json::json!(insights.failing_tools),
            );
        }

        GatekeeperDecision {
            inject_list,
            should_write_candidate,
            hit_refs,
            validate_plans,
            reasons,
            signals,
        }
    }
}

fn to_inject_item(m: &SearchMatch) -> InjectItem {
    InjectItem {
        qa_id: m.qa_id.clone(),
        question: m.question.clone(),
        answer: m.answer.clone(),
        summary: m.summary.clone(),
        trust: m.trust,
        validation_level: m.validation_level,
        score: m.score,
        tags: m.tags.clone(),
    }
}

fn is_stale(m: &SearchMatch, now: DateTime<Utc>) -> bool {
    let Some(s) = &m.expiry_at else {
        return false;
    };
    match DateTime::parse_from_rfc3339(s) {
        Ok(dt) => dt.with_timezone(&Utc) <= now,
        Err(_) => false,
    }
}

fn extract_i32(meta: &Value, key: &str) -> Option<i32> {
    meta.get(key).and_then(|v| {
        if v.is_i64() {
            v.as_i64().map(|x| x as i32)
        } else if v.is_u64() {
            v.as_u64().map(|x| x as i32)
        } else if v.is_string() {
            v.as_str()?.parse::<i32>().ok()
        } else {
            None
        }
    })
}

fn digest_cheap(s: &str, head_chars: usize, tail_chars: usize) -> Value {
    let len = s.len();
    let head = s.chars().take(head_chars).collect::<String>();
    let tail = s
        .chars()
        .rev()
        .take(tail_chars)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();

    serde_json::json!({ "len": len, "head": head, "tail": tail })
}
