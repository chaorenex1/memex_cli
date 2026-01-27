use std::collections::BTreeSet;
use std::sync::OnceLock;

use regex::Regex;
use serde_json::Value;

use crate::tool_event::ToolEvent;

// Cached regex for QA_REF extraction (compiled once, reused forever)
static QA_REF_REGEX: OnceLock<Regex> = OnceLock::new();

fn qa_ref_regex() -> &'static Regex {
    QA_REF_REGEX.get_or_init(|| {
        Regex::new(r"\[QA_REF\s+([A-Za-z0-9_\-]+)\]").expect("QA_REF_REGEX is valid")
    })
}

pub fn extract_qa_refs(text: &str) -> Vec<String> {
    let re = qa_ref_regex();
    let mut set = BTreeSet::new();

    for cap in re.captures_iter(text) {
        if let Some(m) = cap.get(1) {
            set.insert(m.as_str().to_string());
        }
    }

    set.into_iter().collect()
}

pub fn extract_qa_refs_from_tool_events(events: &Vec<ToolEvent>) -> Vec<String> {
    let mut qa_ids = BTreeSet::new();

    for e in events {
        if let Some(output) = &e.output {
            let refs = extract_qa_refs(Value::to_string(output).as_str());
            for r in refs {
                qa_ids.insert(r);
            }
        }
    }
    qa_ids.into_iter().collect()
}

/// Extract the complete final answer from tool events.
///
/// Collects all `assistant.output` events (streaming fragments) and
/// concatenates them into the complete final answer.
pub fn extract_final_answer_from_tool_events(events: &[ToolEvent]) -> String {
    use crate::tool_event::stream_json::EVENT_TYPE_ASSISTANT_OUTPUT;
    use crate::tool_event::stream_json::EVENT_TYPE_EVENT_END;
    use crate::tool_event::stream_json::EVENT_TYPE_TOOL_RESULT;

    let mut parts = Vec::new();

    for e in events {
        if e.event_type == EVENT_TYPE_TOOL_RESULT {
            if let Some(output) = &e.output {
                if let Some(s) = output.as_str() {
                    if !s.is_empty() {
                        parts.push(s);
                    }
                }
            }
        }
        if e.event_type == EVENT_TYPE_ASSISTANT_OUTPUT {
            if let Some(output) = &e.output {
                if let Some(s) = output.as_str() {
                    if !s.is_empty() {
                        parts.push(s);
                    }
                }
            }
        }
        if e.event_type == EVENT_TYPE_EVENT_END {
            if let Some(output) = &e.output {
                if let Some(s) = output.as_str() {
                    if !s.is_empty() {
                        parts.push(s);
                    }
                }
            }
        }
    }

    if parts.is_empty() {
        String::new()
    } else {
        parts.join("")
    }
}

pub fn extract_final_reasoning_from_tool_events(events: &[ToolEvent]) -> String {
    use crate::tool_event::stream_json::EVENT_TYPE_ASSISTANT_REASONING;

    let mut parts = Vec::new();

    for e in events {
        if e.event_type == EVENT_TYPE_ASSISTANT_REASONING {
            if let Some(output) = &e.output {
                if let Some(s) = output.as_str() {
                    if !s.is_empty() {
                        parts.push(s);
                    }
                }
            }
        }
    }

    if parts.is_empty() {
        String::new()
    } else {
        parts.join("")
    }
}
