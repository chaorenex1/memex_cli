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

    for key in ["session_id", "sessionId", "run_id", "runId"] {
        if let Some(id) = v.get(key).and_then(|x| x.as_str()) {
            let id = id.trim();
            if !id.is_empty() {
                return Some(id.to_string());
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::extract_run_id_from_line;

    #[test]
    fn extracts_gemini_session_id_from_init() {
        let line = r#"{"type":"init","timestamp":"2025-12-26T12:48:29.765Z","session_id":"dfa4182a-d2da-4dc7-9080-fa2d39bba588","model":"auto-gemini-2.5"}"#;
        assert_eq!(
            extract_run_id_from_line(line).as_deref(),
            Some("dfa4182a-d2da-4dc7-9080-fa2d39bba588")
        );
    }

    #[test]
    fn ignores_non_json_lines() {
        assert!(extract_run_id_from_line("event: message_start").is_none());
        assert!(extract_run_id_from_line("YOLO mode is enabled.").is_none());
    }
}
