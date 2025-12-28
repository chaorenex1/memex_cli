use std::collections::HashMap;

use serde_json::Value;

use crate::tool_event::ToolEvent;

/// Parses "stream-json" style lines emitted by external CLIs (e.g. codex/claude/gemini).
///
/// It is intentionally best-effort:
/// - Ignores non-JSON lines.
/// - Maps known shapes into the internal ToolEvent schema.
#[derive(Default)]
pub struct StreamJsonToolEventParser {
    // Some formats emit tool_result without repeating tool_name; keep a short-lived mapping.
    pending_tool_name_by_id: HashMap<String, String>,
}

impl StreamJsonToolEventParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn parse_line(&mut self, line: &str) -> Option<ToolEvent> {
        let s = line.trim();
        if !(s.starts_with('{') && s.ends_with('}')) {
            return None;
        }

        let v: Value = serde_json::from_str(s).ok()?;

        // Claude stream-json
        // Shape examples (simplified):
        // - {"type":"assistant","message":{"content":[{"type":"tool_use","id":"...","name":"TodoWrite","input":{...}}]}}
        // - {"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"...","content":"..."}]}}
        if v.get("type").and_then(|x| x.as_str()) == Some("assistant") {
            if let Some(items) = v
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_array())
            {
                for item in items {
                    if item.get("type").and_then(|x| x.as_str()) != Some("tool_use") {
                        continue;
                    }

                    let id = item
                        .get("id")
                        .and_then(|x| x.as_str())
                        .map(|x| x.to_string());
                    let tool = item
                        .get("name")
                        .and_then(|x| x.as_str())
                        .map(|x| x.to_string());
                    let args = item.get("input").cloned().unwrap_or(Value::Null);

                    return Some(ToolEvent {
                        v: 1,
                        event_type: "tool.request".to_string(),
                        ts: None,
                        run_id: None,
                        id,
                        tool,
                        action: None,
                        args,
                        ok: None,
                        output: None,
                        error: None,
                        rationale: None,
                    });
                }
            }
        }

        if v.get("type").and_then(|x| x.as_str()) == Some("user") {
            if let Some(items) = v
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_array())
            {
                for item in items {
                    if item.get("type").and_then(|x| x.as_str()) != Some("tool_result") {
                        continue;
                    }

                    let id = item
                        .get("tool_use_id")
                        .and_then(|x| x.as_str())
                        .map(|x| x.to_string());

                    // Claude doesn't always expose an explicit ok/error flag here.
                    // Best-effort: treat presence of tool_use_result.isError as authoritative,
                    // otherwise fall back to "has content".
                    let ok = v
                        .get("tool_use_result")
                        .and_then(|r| r.get("isError").or_else(|| r.get("is_error")))
                        .and_then(|x| x.as_bool())
                        .map(|is_error| !is_error)
                        .or_else(|| {
                            if item.get("content").is_some() {
                                Some(true)
                            } else {
                                None
                            }
                        });

                    let output = item
                        .get("content")
                        .cloned()
                        .or_else(|| v.get("tool_use_result").cloned());

                    return Some(ToolEvent {
                        v: 1,
                        event_type: "tool.result".to_string(),
                        ts: None,
                        run_id: None,
                        id,
                        tool: None,
                        action: None,
                        args: Value::Null,
                        ok,
                        output,
                        error: None,
                        rationale: None,
                    });
                }
            }
        }

        // Gemini stream-json
        if v.get("type").and_then(|x| x.as_str()) == Some("tool_use") {
            let tool = v
                .get("tool_name")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            let id = v
                .get("tool_id")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            let ts = v
                .get("timestamp")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            let args = v.get("parameters").cloned().unwrap_or(Value::Null);

            if let (Some(id), Some(tool)) = (id.clone(), tool.clone()) {
                self.pending_tool_name_by_id.insert(id, tool);
            }

            return Some(ToolEvent {
                v: 1,
                event_type: "tool.request".to_string(),
                ts,
                run_id: None,
                id,
                tool,
                action: None,
                args,
                ok: None,
                output: None,
                error: None,
                rationale: None,
            });
        }

        if v.get("type").and_then(|x| x.as_str()) == Some("tool_result") {
            let id = v
                .get("tool_id")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            let ts = v
                .get("timestamp")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            let ok = match v.get("status").and_then(|x| x.as_str()) {
                Some("success") => Some(true),
                Some("error") => Some(false),
                _ => None,
            };
            let output = v.get("output").cloned();

            let tool = id
                .as_ref()
                .and_then(|tid| self.pending_tool_name_by_id.get(tid).cloned());

            return Some(ToolEvent {
                v: 1,
                event_type: "tool.result".to_string(),
                ts,
                run_id: None,
                id,
                tool,
                action: None,
                args: Value::Null,
                ok,
                output,
                error: None,
                rationale: None,
            });
        }

        // Codex stream-json
        if let Some(item) = v.get("item") {
            if item.get("type").and_then(|x| x.as_str()) == Some("mcp_tool_call") {
                let line_type = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
                let id = item
                    .get("id")
                    .and_then(|x| x.as_str())
                    .map(|x| x.to_string());

                let tool = item
                    .get("tool")
                    .and_then(|x| x.as_str())
                    .map(|x| x.to_string());
                let server = item
                    .get("server")
                    .and_then(|x| x.as_str())
                    .map(|x| x.to_string());
                let tool = match (server, tool) {
                    (Some(s), Some(t)) => Some(format!("{s}.{t}")),
                    (_, t) => t,
                };

                let args = item.get("arguments").cloned().unwrap_or(Value::Null);

                if line_type == "item.started" {
                    return Some(ToolEvent {
                        v: 1,
                        event_type: "tool.request".to_string(),
                        ts: None,
                        run_id: None,
                        id,
                        tool,
                        action: None,
                        args,
                        ok: None,
                        output: None,
                        error: None,
                        rationale: None,
                    });
                }

                if line_type == "item.completed" {
                    let status = item.get("status").and_then(|x| x.as_str());
                    let ok = match status {
                        Some("completed") => Some(true),
                        Some("failed") => Some(false),
                        _ => None,
                    };

                    let output = item.get("result").cloned();
                    let error = item
                        .get("error")
                        .and_then(|x| x.as_str())
                        .map(|x| x.to_string());

                    return Some(ToolEvent {
                        v: 1,
                        event_type: "tool.result".to_string(),
                        ts: None,
                        run_id: None,
                        id,
                        tool,
                        action: None,
                        args: Value::Null,
                        ok,
                        output,
                        error,
                        rationale: None,
                    });
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::StreamJsonToolEventParser;

    #[test]
    fn parses_gemini_tool_use_and_result_with_tool_name_tracking() {
        let mut p = StreamJsonToolEventParser::new();

        let use_line = r#"{"type":"tool_use","timestamp":"2025-12-26T12:48:36.765Z","tool_name":"run_shell_command","tool_id":"run_shell_command-1766753316765-e8db","parameters":{"command":"echo hi"}}"#;
        let ev1 = p.parse_line(use_line).expect("tool_use should parse");
        assert_eq!(ev1.event_type, "tool.request");
        assert_eq!(ev1.tool.as_deref(), Some("run_shell_command"));
        assert_eq!(
            ev1.id.as_deref(),
            Some("run_shell_command-1766753316765-e8db")
        );

        let result_line = r#"{"type":"tool_result","timestamp":"2025-12-26T12:48:38.811Z","tool_id":"run_shell_command-1766753316765-e8db","status":"success","output":""}"#;
        let ev2 = p.parse_line(result_line).expect("tool_result should parse");
        assert_eq!(ev2.event_type, "tool.result");
        assert_eq!(ev2.ok, Some(true));
        // tool_name is recovered from prior tool_use
        assert_eq!(ev2.tool.as_deref(), Some("run_shell_command"));
    }

    #[test]
    fn parses_codex_mcp_tool_call_started_and_completed() {
        let mut p = StreamJsonToolEventParser::new();

        let started = r#"{"type":"item.started","item":{"id":"item_1","type":"mcp_tool_call","server":"aduib-mcp-sever","tool":"retrieve_qa_kb","arguments":{"query":"x"},"status":"in_progress"}}"#;
        let ev1 = p.parse_line(started).expect("item.started should parse");
        assert_eq!(ev1.event_type, "tool.request");
        assert_eq!(ev1.id.as_deref(), Some("item_1"));
        assert_eq!(ev1.tool.as_deref(), Some("aduib-mcp-sever.retrieve_qa_kb"));

        let completed = r#"{"type":"item.completed","item":{"id":"item_1","type":"mcp_tool_call","server":"aduib-mcp-sever","tool":"retrieve_qa_kb","arguments":{"query":"x"},"result":{"content":[{"type":"text","text":"ok"}]},"error":null,"status":"completed"}}"#;
        let ev2 = p
            .parse_line(completed)
            .expect("item.completed should parse");
        assert_eq!(ev2.event_type, "tool.result");
        assert_eq!(ev2.ok, Some(true));
        assert_eq!(ev2.id.as_deref(), Some("item_1"));
        assert_eq!(ev2.tool.as_deref(), Some("aduib-mcp-sever.retrieve_qa_kb"));
        assert!(ev2.output.is_some());
    }

    #[test]
    fn ignores_non_json_lines() {
        let mut p = StreamJsonToolEventParser::new();
        assert!(p.parse_line("event: message_start").is_none());
        assert!(p.parse_line("Could not parse message into JSON:").is_none());
    }

    #[test]
    fn parses_claude_tool_use_envelope() {
        let mut p = StreamJsonToolEventParser::new();

        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"call_00_abc","name":"TodoWrite","input":{"todos":[{"content":"x"}]}}]}}"#;
        let ev = p.parse_line(line).expect("claude tool_use should parse");
        assert_eq!(ev.event_type, "tool.request");
        assert_eq!(ev.id.as_deref(), Some("call_00_abc"));
        assert_eq!(ev.tool.as_deref(), Some("TodoWrite"));
        assert!(ev.args.get("todos").is_some());
    }

    #[test]
    fn parses_claude_tool_result_envelope() {
        let mut p = StreamJsonToolEventParser::new();

        let line = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"call_00_abc","content":"ok"}]}}"#;
        let ev = p.parse_line(line).expect("claude tool_result should parse");
        assert_eq!(ev.event_type, "tool.result");
        assert_eq!(ev.id.as_deref(), Some("call_00_abc"));
        assert_eq!(ev.ok, Some(true));
        assert!(ev.output.is_some());
    }
}
