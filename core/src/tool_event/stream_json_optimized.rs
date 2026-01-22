use std::collections::HashMap;
use std::time::Instant;

use chrono::Local;

use serde_json::Value;

use crate::tool_event::ToolEvent;

/// Optimized parser for "stream-json" style lines emitted by external CLIs.
///
/// Performance optimizations:
/// - Uses match statement for type dispatch (better branch prediction)
/// - Extracts type once and reuses the value
/// - Inline helpers for common operations
pub struct StreamJsonToolEventParser {
    pending_tool_name_by_id: HashMap<String, String>,
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

    #[inline]
    fn current_ts(&mut self) -> &str {
        const REFRESH_INTERVAL_MS: u128 = 50;
        if self.last_ts_refresh.elapsed().as_millis() >= REFRESH_INTERVAL_MS {
            self.cached_ts = Local::now().to_rfc3339();
            self.last_ts_refresh = Instant::now();
        }
        &self.cached_ts
    }

    /// Optimized parse_value with better branch prediction via match
    pub fn parse_value(&mut self, v: &Value) -> Option<ToolEvent> {
        let ts = Some(self.current_ts().to_string());

        // Extract type once for better performance
        let type_str = v.get("type")?.as_str()?;

        // Use match for better compiler optimization and branch prediction
        match type_str {
            "system" => self.handle_system_type(v, ts),
            "result" => self.handle_result_type(v, ts),
            "assistant" => self.handle_assistant_type(v, ts),
            "user" => self.handle_user_type(v, ts),
            "init" => self.handle_init_type(v, ts),
            "message" => self.handle_message_type(v, ts),
            "tool_use" => self.handle_tool_use_type(v, ts),
            "tool_result" => self.handle_tool_result_type(v, ts),
            "turn.started" | "turn.completed" => self.handle_turn_type(v, ts, type_str),
            "item.started" | "item.completed" => self.handle_item_type(v, ts, type_str),
            _ => None,
        }
    }

    #[inline]
    fn handle_system_type(&self, v: &Value, ts: Option<String>) -> Option<ToolEvent> {
        if v.get("subtype").is_none() {
            return None;
        }

        let session_id = v.get("session_id")
            .and_then(|x| x.as_str())
            .map(|x| x.to_string());
        let subtype = v.get("subtype")
            .and_then(|x| x.as_str())
            .unwrap_or("");

        Some(ToolEvent {
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
        })
    }

    #[inline]
    fn handle_result_type(&self, v: &Value, ts: Option<String>) -> Option<ToolEvent> {
        if v.get("subtype").is_none() {
            return None;
        }

        let subtype = v.get("subtype")
            .and_then(|x| x.as_str())
            .unwrap_or("");
        let result = v.get("result").cloned().unwrap_or(Value::Null);
        let is_error = v.get("is_error")
            .and_then(|x| x.as_bool())
            .unwrap_or(false);

        Some(ToolEvent {
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
        })
    }

    fn handle_assistant_type(&mut self, v: &Value, ts: Option<String>) -> Option<ToolEvent> {
        let assistant_message = v.get("message")?.as_object()?;
        let default_message = Value::String("{}".to_string());
        let message = assistant_message.get("message").unwrap_or(&default_message);
        let items = assistant_message.get("content")?.as_array()?;

        for item in items {
            let item_type = item.get("type")?.as_str()?;

            match item_type {
                "tool_use" => {
                    let id = item.get("id")
                        .and_then(|x| x.as_str())
                        .map(|x| x.to_string());
                    let tool = item.get("name")
                        .and_then(|x| x.as_str())
                        .map(|x| x.to_string());
                    let args = item.get("input").cloned().unwrap_or(Value::Null);

                    if let (Some(ref id_val), Some(ref tool_val)) = (id, tool) {
                        self.pending_tool_name_by_id.insert(id_val.clone(), tool_val.clone());
                    }

                    return Some(ToolEvent {
                        v: 1,
                        event_type: "tool.request".to_string(),
                        ts,
                        run_id: None,
                        id,
                        tool,
                        action: item.get("name")
                            .and_then(|x| x.as_str())
                            .map(|x| x.to_string()),
                        args: args.clone(),
                        ok: None,
                        output: Some(args),
                        error: None,
                        rationale: None,
                    });
                }
                "text" => {
                    let mut content = item.get("text")
                        .and_then(|x| x.as_str())
                        .map(|x| x.to_string())
                        .unwrap_or_default();
                    if content == "(no content)" {
                        content.clear();
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
                "thinking" => {
                    let mut content = item.get("thinking")
                        .and_then(|x| x.as_str())
                        .map(|x| x.to_string())
                        .unwrap_or_default();
                    if content == "(no content)" {
                        content.clear();
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
                _ => {}
            }
        }
        None
    }

    fn handle_user_type(&mut self, v: &Value, ts: Option<String>) -> Option<ToolEvent> {
        let user_message = v.get("message")?.as_object()?;
        let default_message = Value::String("{}".to_string());
        let message = user_message.get("message").unwrap_or(&default_message);
        let items = user_message.get("content")?.as_array()?;

        for item in items {
            if item.get("type")?.as_str()? == "tool_result" {
                let id = item.get("tool_use_id")
                    .and_then(|x| x.as_str())
                    .map(|x| x.to_string());

                let ok = v.get("tool_use_result")
                    .and_then(|r| r.get("isError").or_else(|| r.get("is_error")))
                    .and_then(|x| x.as_bool())
                    .map(|is_error| !is_error)
                    .or_else(|| if item.get("content").is_some() { Some(true) } else { None });

                let output = item.get("content").cloned()
                    .or_else(|| v.get("tool_use_result").cloned());

                let tool = id.as_ref()
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

            if item.get("type")?.as_str()? == "text" {
                let mut content = item.get("text")
                    .and_then(|x| x.as_str())
                    .map(|x| x.to_string())
                    .unwrap_or_default();
                if content == "(no content)" {
                    content.clear();
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
        None
    }

    #[inline]
    fn handle_init_type(&self, v: &Value, ts: Option<String>) -> Option<ToolEvent> {
        Some(ToolEvent {
            v: 1,
            event_type: "event.start".to_string(),
            ts,
            run_id: None,
            id: None,
            tool: None,
            action: Some("init".to_string()),
            args: Value::Null,
            ok: None,
            output: Some(Value::String(v.to_string())),
            error: None,
            rationale: None,
        })
    }

    #[inline]
    fn handle_message_type(&self, v: &Value, ts: Option<String>) -> Option<ToolEvent> {
        let role = v.get("role")?.as_str()?;

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
        None
    }

    fn handle_tool_use_type(&mut self, v: &Value, ts: Option<String>) -> Option<ToolEvent> {
        let tool = v.get("tool_name")
            .and_then(|x| x.as_str())
            .map(|x| x.to_string());
        let id = v.get("tool_id")
            .and_then(|x| x.as_str())
            .map(|x| x.to_string());
        let ts = v.get("timestamp")
            .and_then(|x| x.as_str())
            .map(|x| x.to_string());
        let args = v.get("parameters").cloned().unwrap_or(Value::Null);

        if let (Some(ref id_val), Some(ref tool_val)) = (id, tool) {
            self.pending_tool_name_by_id.insert(id_val.clone(), tool_val.clone());
        }

        Some(ToolEvent {
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
        })
    }

    fn handle_tool_result_type(&mut self, v: &Value, ts: Option<String>) -> Option<ToolEvent> {
        let id = v.get("tool_id")
            .and_then(|x| x.as_str())
            .map(|x| x.to_string());
        let ts = v.get("timestamp")
            .and_then(|x| x.as_str())
            .map(|x| x.to_string());
        let ok = match v.get("status").and_then(|x| x.as_str()) {
            Some("success") => Some(true),
            Some("error") => Some(false),
            _ => None,
        };
        let output = v.get("output").cloned();

        let tool = id.as_ref()
            .and_then(|tid| self.pending_tool_name_by_id.get(tid).cloned());

        Some(ToolEvent {
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
        })
    }

    #[inline]
    fn handle_turn_type(&self, v: &Value, ts: Option<String>, type_str: &str) -> Option<ToolEvent> {
        match type_str {
            "turn.started" => Some(ToolEvent {
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
            }),
            "turn.completed" => {
                let usage = v.get("usage").cloned().unwrap_or(Value::Null);
                Some(ToolEvent {
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
                })
            }
            _ => None,
        }
    }

    fn handle_item_type(&mut self, v: &Value, ts: Option<String>, line_type: &str) -> Option<ToolEvent> {
        let item = v.get("item")?;
        let item_type = item.get("type")?.as_str()?;

        match item_type {
            "mcp_tool_call" => self.handle_mcp_tool_call(item, ts, line_type),
            "agent_message" if line_type == "item.completed" => self.handle_agent_message(item, ts),
            "reasoning" if line_type == "item.completed" => self.handle_reasoning(item, ts),
            "command_execution" => self.handle_command_execution(item, ts, line_type),
            _ => None,
        }
    }

    fn handle_mcp_tool_call(&mut self, item: &Value, ts: Option<String>, line_type: &str) -> Option<ToolEvent> {
        let id = item.get("id")
            .and_then(|x| x.as_str())
            .map(|x| x.to_string());
        let tool = item.get("tool")
            .and_then(|x| x.as_str())
            .map(|x| x.to_string());
        let server = item.get("server")
            .and_then(|x| x.as_str())
            .map(|x| x.to_string());
        let args = item.get("arguments").cloned().unwrap_or(Value::Null);

        match line_type {
            "item.started" => Some(ToolEvent {
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
            }),
            "item.completed" => {
                let status = item.get("status").and_then(|x| x.as_str());
                let ok = match status {
                    Some("completed") => Some(true),
                    Some("failed") => Some(false),
                    _ => None,
                };
                let output = item.get("result").cloned();
                let error = item.get("error")
                    .and_then(|x| x.as_str())
                    .map(|x| x.to_string());

                Some(ToolEvent {
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
                })
            }
            _ => None,
        }
    }

    #[inline]
    fn handle_agent_message(&self, item: &Value, ts: Option<String>) -> Option<ToolEvent> {
        let id = item.get("id")
            .and_then(|x| x.as_str())
            .map(|x| x.to_string());
        let text = item.get("text")
            .and_then(|x| x.as_str())
            .map(|x| x.to_string())
            .unwrap_or_default();

        Some(ToolEvent {
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
        })
    }

    #[inline]
    fn handle_reasoning(&self, item: &Value, ts: Option<String>) -> Option<ToolEvent> {
        let id = item.get("id")
            .and_then(|x| x.as_str())
            .map(|x| x.to_string());
        let text = item.get("text")
            .and_then(|x| x.as_str())
            .map(|x| x.to_string())
            .unwrap_or_default();

        Some(ToolEvent {
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
        })
    }

    fn handle_command_execution(&self, item: &Value, ts: Option<String>, line_type: &str) -> Option<ToolEvent> {
        let id = item.get("id")
            .and_then(|x| x.as_str())
            .map(|x| x.to_string());
        let command = item.get("command").cloned().unwrap_or(Value::Null);

        match line_type {
            "item.started" => Some(ToolEvent {
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
            }),
            "item.completed" => {
                let exit_code = item.get("exit_code").and_then(|x| x.as_i64());
                let ok = exit_code.map(|c| c == 0);
                let output = item.get("aggregated_output").cloned();
                let status = item.get("status")
                    .and_then(|x| x.as_str())
                    .unwrap_or("");
                let error = if status == "failed" {
                    Some("command_execution_failed".to_string())
                } else {
                    None
                };

                Some(ToolEvent {
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
                })
            }
            _ => None,
        }
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
