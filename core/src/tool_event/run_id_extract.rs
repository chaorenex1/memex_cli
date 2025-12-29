use serde_json::Value;

/// Best-effort extraction of a stable run/session id from a JSON line.
///
/// Known formats:
/// - Gemini stream-json: {"type":"init", "session_id":"..."}
/// - Some tools: {"run_id":"..."} or camelCase variants
pub fn extract_run_id_from_line(line: &str) -> Option<String> {
    let s = line.trim();
    if !(s.starts_with('{') && s.ends_with('}')) {
        return None;
    }

    let v: Value = serde_json::from_str(s).ok()?;
    extract_run_id_from_value(&v)
}

pub fn extract_run_id_from_value(v: &Value) -> Option<String> {
    for key in ["session_id", "sessionId", "run_id", "runId", "thread_id"] {
        if let Some(id) = v.get(key).and_then(|x| x.as_str()) {
            let id = id.trim();
            if !id.is_empty() {
                return Some(id.to_string());
            }
        }
    }

    None
}

