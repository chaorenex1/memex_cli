use chrono::Utc;

use crate::gatekeeper::GatekeeperDecision;

use super::models::{QACandidatePayload, QAHitsPayload, QAReferencePayload, QAValidationPayload};
use super::types::CandidateDraft;

pub fn build_hit_payload(project_id: &str, decision: &GatekeeperDecision) -> Option<QAHitsPayload> {
    if decision.hit_refs.is_empty() {
        return None;
    }

    let refs = decision
        .hit_refs
        .iter()
        .map(|r| QAReferencePayload {
            qa_id: r.qa_id.clone(),
            shown: Some(r.shown),
            used: Some(r.used),
            message_id: r.message_id.clone(),
            context: r.context.clone(),
        })
        .collect::<Vec<_>>();

    Some(QAHitsPayload {
        project_id: project_id.to_string(),
        references: refs,
    })
}

pub fn build_validate_payloads(
    project_id: &str,
    decision: &GatekeeperDecision,
) -> Vec<QAValidationPayload> {
    decision
        .validate_plans
        .iter()
        .map(|p| QAValidationPayload {
            project_id: project_id.to_string(),
            qa_id: p.qa_id.clone(),
            result: Some(p.result.clone()),
            signal_strength: Some(p.signal_strength.clone()),
            strong_signal: Some(p.strong_signal),
            context: p.context.clone(),
            ts: Some(Utc::now().to_rfc3339()),
            payload: Some(p.payload.clone()),
            source: Some("mem-codecli".to_string()),
            client: None,
            success: None,
        })
        .collect()
}

pub fn build_candidate_payloads(
    project_id: &str,
    drafts: &[CandidateDraft],
) -> Vec<QACandidatePayload> {
    drafts
        .iter()
        .map(|d| QACandidatePayload {
            project_id: project_id.to_string(),
            question: d.question.clone(),
            answer: d.answer.clone(),
            tags: d.tags.clone(),
            confidence: d.confidence,
            metadata: d.metadata.clone(),
            summary: d.summary.clone(),
            source: d.source.clone(),
            author: None,
        })
        .collect()
}
