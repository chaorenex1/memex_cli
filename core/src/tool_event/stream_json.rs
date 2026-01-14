use std::collections::HashMap;
use std::time::Instant;

use chrono::Local;

use serde_json::Value;

use crate::tool_event::ToolEvent;

/// Parses "stream-json" style lines emitted by external CLIs (e.g. codex/claude/gemini).
///
/// It is intentionally best-effort:
/// - Ignores non-JSON lines.
/// - Maps known shapes into the internal ToolEvent schema.
pub struct StreamJsonToolEventParser {
    // Some formats emit tool_result without repeating tool_name; keep a short-lived mapping.
    pending_tool_name_by_id: HashMap<String, String>,
    // Cached timestamp for performance (refreshed every 50ms)
    cached_ts: String,
    last_ts_refresh: Instant,
}

impl Default for StreamJsonToolEventParser {
    fn default() -> Self {
        Self {
            pending_tool_name_by_id: HashMap::new(),
            cached_ts: Local::now().to_rfc3339(),
            last_ts_refresh: Instant::now(),
        }
    }
}

impl StreamJsonToolEventParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get current timestamp, refreshing cache if stale (>50ms)
    #[inline]
    fn current_ts(&mut self) -> String {
        const REFRESH_INTERVAL_MS: u128 = 50;
        if self.last_ts_refresh.elapsed().as_millis() >= REFRESH_INTERVAL_MS {
            self.cached_ts = Local::now().to_rfc3339();
            self.last_ts_refresh = Instant::now();
        }
        self.cached_ts.clone()
    }

    pub fn parse_value(&mut self, v: &Value) -> Option<ToolEvent> {
        let ts = Some(self.current_ts());
        // Claude stream-json
        // Shape examples (simplified):
        // - {"type":"assistant","message":{"content":[{"type":"tool_use","id":"...","name":"TodoWrite","input":{...}}]}}
        // - {"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"...","content":"..."}]}}
        if v.get("type").and_then(|x| x.as_str()) == Some("system") && v.get("subtype").is_some() {
            let session_id = v
                .get("session_id")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            let subtype = v.get("subtype").and_then(|x| x.as_str()).unwrap_or("");
            return Some(ToolEvent {
                v: 1,
                event_type: "event.start".to_string(),
                ts,
                run_id: session_id,
                id: None,
                tool: None,
                action: Some(subtype.to_string()),
                args: Value::Null,
                ok: None,
                output: Some(Value::String(v.to_string())),
                error: None,
                rationale: None,
            });
        }
        if v.get("type").and_then(|x| x.as_str()) == Some("result") && v.get("subtype").is_some() {
            let subtype = v.get("subtype").and_then(|x| x.as_str()).unwrap_or("");
            let result = v.get("result").cloned().unwrap_or(Value::Null);
            let is_error = v.get("is_error").and_then(|x| x.as_bool()).unwrap_or(false);
            return Some(ToolEvent {
                v: 1,
                event_type: "event.end".to_string(),
                ts,
                run_id: None,
                id: None,
                tool: None,
                action: Some(subtype.to_string()),
                args: Value::Null,
                ok: Some(is_error),
                output: Some(Value::String(result.to_string())),
                error: None,
                rationale: None,
            });
        }
        if v.get("type").and_then(|x| x.as_str()) == Some("assistant") {
            if let Some(assistant_message) = v.get("message").and_then(|c| c.as_object()) {
                let default_message = Value::String("{}".to_string());
                let message = assistant_message.get("message").unwrap_or(&default_message);
                let items = assistant_message
                    .get("content")
                    .and_then(|c| c.as_array())?;
                for item in items {
                    if item.get("type").and_then(|x| x.as_str()) == Some("tool_use") {
                        let id = item
                            .get("id")
                            .and_then(|x| x.as_str())
                            .map(|x| x.to_string());
                        let tool = item
                            .get("name")
                            .and_then(|x| x.as_str())
                            .map(|x| x.to_string());
                        let args = item.get("input").cloned().unwrap_or(Value::Null);
                        let name = item.get("name").and_then(|x| x.as_str());

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
                            action: name.map(|x| x.to_string()),
                            args: args.clone(),
                            ok: None,
                            output: Some(args.clone()),
                            error: None,
                            rationale: None,
                        });
                    }

                    if item.get("type").and_then(|x| x.as_str()) == Some("text") {
                        let mut content = item
                            .get("text")
                            .and_then(|x| x.as_str())
                            .map(|x| x.to_string())
                            .unwrap_or_default();
                        if content == "(no content)" {
                            content = "".to_string();
                        }
                        return Some(ToolEvent {
                            v: 1,
                            event_type: "assistant.output".to_string(),
                            ts,
                            run_id: None,
                            id: None,
                            tool: None,
                            action: Some(message.to_string()),
                            args: Value::Null,
                            ok: None,
                            output: Some(Value::String(content)),
                            error: None,
                            rationale: None,
                        });
                    }

                    if item.get("type").and_then(|x| x.as_str()) == Some("thinking") {
                        let mut content = item
                            .get("thinking")
                            .and_then(|x| x.as_str())
                            .map(|x| x.to_string())
                            .unwrap_or_default();
                        if content == "(no content)" {
                            content = "".to_string();
                        }
                        return Some(ToolEvent {
                            v: 1,
                            event_type: "assistant.reasoning".to_string(),
                            ts,
                            run_id: None,
                            id: None,
                            tool: None,
                            action: Some(message.to_string()),
                            args: Value::Null,
                            ok: None,
                            output: Some(Value::String(content)),
                            error: None,
                            rationale: None,
                        });
                    }
                }
            }
        }

        if v.get("type").and_then(|x| x.as_str()) == Some("user") {
            if let Some(user_message) = v.get("message").and_then(|c| c.as_object()) {
                let default_message = Value::String("{}".to_string());
                let message = user_message.get("message").unwrap_or(&default_message);
                let items = user_message.get("content").and_then(|c| c.as_array())?;
                for item in items {
                    if item.get("type").and_then(|x| x.as_str()) == Some("tool_result") {
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
                            action: Some(message.to_string()),
                            args: Value::Null,
                            ok,
                            output,
                            error: None,
                            rationale: None,
                        });
                    }

                    if item.get("type").and_then(|x| x.as_str()) == Some("text") {
                        let mut content = item
                            .get("text")
                            .and_then(|x| x.as_str())
                            .map(|x| x.to_string())
                            .unwrap_or_default();
                        if content == "(no content)" {
                            content = "".to_string();
                        }
                        return Some(ToolEvent {
                            v: 1,
                            event_type: "assistant.output".to_string(),
                            ts,
                            run_id: None,
                            id: None,
                            tool: None,
                            action: Some(message.to_string()),
                            args: Value::Null,
                            ok: None,
                            output: Some(Value::String(content)),
                            error: None,
                            rationale: None,
                        });
                    }
                }
            }
        }

        // Gemini stream-json
        if v.get("type").and_then(|x| x.as_str()) == Some("init") {
            let type_ = v.get("type").and_then(|x| x.as_str()).unwrap_or("");

            return Some(ToolEvent {
                v: 1,
                event_type: "event.start".to_string(),
                ts,
                run_id: None,
                id: None,
                tool: None,
                action: Some(type_.to_string()),
                args: Value::Null,
                ok: None,
                output: Some(Value::String(v.to_string())),
                error: None,
                rationale: None,
            });
        }

        if v.get("type").and_then(|x| x.as_str()) == Some("result") {
            let type_ = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
            let status = v
                .get("status")
                .and_then(|x| x.as_str())
                .map(|s| s == "success");
            let stats = v.get("stats").cloned().unwrap_or(Value::Null);

            return Some(ToolEvent {
                v: 1,
                event_type: "event.end".to_string(),
                ts,
                run_id: None,
                id: None,
                tool: None,
                action: Some(type_.to_string()),
                args: Value::Null,
                ok: status,
                output: Some(stats),
                error: None,
                rationale: None,
            });
        }

        if v.get("type").and_then(|x| x.as_str()) == Some("message") {
            let role = v.get("role").and_then(|x| x.as_str()).unwrap_or("");

            if role == "assistant" {
                let content = v.get("content").cloned().unwrap_or(Value::Null);

                return Some(ToolEvent {
                    v: 1,
                    event_type: "assistant.output".to_string(),
                    ts,
                    run_id: None,
                    id: None,
                    tool: None,
                    action: Some(role.to_string()),
                    args: Value::Null,
                    ok: None,
                    output: Some(content),
                    error: None,
                    rationale: None,
                });
            }
        }

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

        if let Some(turn) = v.get("type").and_then(|x| x.as_str()) {
            if turn == "turn.started" {
                return Some(ToolEvent {
                    v: 1,
                    event_type: "event.start".to_string(),
                    ts,
                    run_id: None,
                    id: None,
                    tool: None,
                    action: None,
                    args: Value::Null,
                    ok: None,
                    output: None,
                    error: None,
                    rationale: None,
                });
            } else if turn == "turn.completed" {
                let usage = v.get("usage").cloned().unwrap_or(Value::Null);
                return Some(ToolEvent {
                    v: 1,
                    event_type: "event.end".to_string(),
                    ts,
                    run_id: None,
                    id: None,
                    tool: None,
                    action: None,
                    args: Value::Null,
                    ok: Some(true),
                    output: Some(usage),
                    error: None,
                    rationale: None,
                });
            }
        }
        // Codex stream-json
        if let Some(item) = v.get("item") {
            // Codex tool calls
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
                // let tool = match (server, tool) {
                //     (Some(s), Some(t)) => Some(format!("{s}.{t}")),
                //     (_, t) => t,
                // };

                let args = item.get("arguments").cloned().unwrap_or(Value::Null);

                if line_type == "item.started" {
                    return Some(ToolEvent {
                        v: 1,
                        event_type: "tool.request".to_string(),
                        ts,
                        run_id: None,
                        id,
                        tool: server,
                        action: tool,
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
                        ts,
                        run_id: None,
                        id,
                        tool: server,
                        action: tool,
                        args,
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
                    ts,
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
                    ts,
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
                        ts,
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
                        ts,
                        run_id: None,
                        id,
                        tool: Some("command_execution".to_string()),
                        action: Some("exec".to_string()),
                        args: serde_json::json!({ "command": command }),
                        ok,
                        output: output.or_else(|| {
                            Some(serde_json::json!({ "exit_code": exit_code, "status": status }))
                        }),
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
