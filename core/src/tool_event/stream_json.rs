use std::collections::HashMap;

use serde_json::Value;

use crate::tool_event::ToolEvent;

pub fn extract_assistant_text_from_stream_json_line(line: &str) -> Option<String> {
    let s = line.trim();
    if !(s.starts_with('{') && s.ends_with('}')) {
        return None;
    }

    let v: Value = serde_json::from_str(s).ok()?;

    // Claude stream-json: assistant message with content items like:
    // {"type":"assistant","message":{"content":[{"type":"text","text":"..."}]}}
    if v.get("type").and_then(|x| x.as_str()) == Some("assistant") {
        let items = v
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array())?;

        for item in items {
            let ty = item.get("type").and_then(|x| x.as_str()).unwrap_or("");
            if ty != "text" && ty != "output_text" {
                continue;
            }
            if let Some(t) = item.get("text").and_then(|x| x.as_str()) {
                if !t.is_empty() {
                    return Some(t.to_string());
                }
            }
        }
    }

    None
}

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

    pub fn parse_value(&mut self, v: &Value) -> Option<ToolEvent> {
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

            // Codex agent output (assistant text)
            if v.get("type").and_then(|x| x.as_str()) == Some("item.completed")
                && item.get("type").and_then(|x| x.as_str()) == Some("agent_message")
            {
                let id = item
                    .get("id")
                    .and_then(|x| x.as_str())
                    .map(|x| x.to_string());
                let text = item
                    .get("text")
                    .and_then(|x| x.as_str())
                    .map(|x| x.to_string())
                    .unwrap_or_default();

                return Some(ToolEvent {
                    v: 1,
                    event_type: "assistant.output".to_string(),
                    ts: None,
                    run_id: None,
                    id,
                    tool: None,
                    action: None,
                    args: Value::Null,
                    ok: None,
                    output: Some(Value::String(text)),
                    error: None,
                    rationale: None,
                });
            }

            // Codex reasoning trace (optional; keep structured but not treated as tool.*)
            if v.get("type").and_then(|x| x.as_str()) == Some("item.completed")
                && item.get("type").and_then(|x| x.as_str()) == Some("reasoning")
            {
                let id = item
                    .get("id")
                    .and_then(|x| x.as_str())
                    .map(|x| x.to_string());
                let text = item
                    .get("text")
                    .and_then(|x| x.as_str())
                    .map(|x| x.to_string())
                    .unwrap_or_default();

                return Some(ToolEvent {
                    v: 1,
                    event_type: "assistant.reasoning".to_string(),
                    ts: None,
                    run_id: None,
                    id,
                    tool: None,
                    action: None,
                    args: Value::Null,
                    ok: None,
                    output: Some(Value::String(text)),
                    error: None,
                    rationale: None,
                });
            }

            // Codex local command execution (best-effort mapping)
            let is_cmd = item.get("type").and_then(|x| x.as_str()) == Some("command_execution");
            if is_cmd {
                let line_type = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
                let id = item
                    .get("id")
                    .and_then(|x| x.as_str())
                    .map(|x| x.to_string());
                let command = item.get("command").cloned().unwrap_or(Value::Null);

                if line_type == "item.started" {
                    return Some(ToolEvent {
                        v: 1,
                        event_type: "tool.request".to_string(),
                        ts: None,
                        run_id: None,
                        id,
                        tool: Some("command_execution".to_string()),
                        action: Some("exec".to_string()),
                        args: serde_json::json!({ "command": command }),
                        ok: None,
                        output: None,
                        error: None,
                        rationale: None,
                    });
                }

                if line_type == "item.completed" {
                    let exit_code = item.get("exit_code").and_then(|x| x.as_i64());
                    let ok = exit_code.map(|c| c == 0);
                    let output = item.get("aggregated_output").cloned();
                    let status = item.get("status").and_then(|x| x.as_str()).unwrap_or("");
                    let error = if status == "failed" {
                        Some("command_execution_failed".to_string())
                    } else {
                        None
                    };

                    return Some(ToolEvent {
                        v: 1,
                        event_type: "tool.result".to_string(),
                        ts: None,
                        run_id: None,
                        id,
                        tool: Some("command_execution".to_string()),
                        action: Some("exec".to_string()),
                        args: Value::Null,
                        ok,
                        output: output.or_else(|| Some(serde_json::json!({ "exit_code": exit_code, "status": status }))),
                        error,
                        rationale: None,
                    });
                }
            }
        }

        None
    }

    pub fn parse_line(&mut self, line: &str) -> Option<ToolEvent> {
        let s = line.trim();
        if !(s.starts_with('{') && s.ends_with('}')) {
            return None;
        }

        let v: Value = serde_json::from_str(s).ok()?;
        self.parse_value(&v)
    }
}
