use std::collections::HashMap;
use std::time::Instant;

use chrono::Local;

use serde_json::Value;

use crate::tool_event::ToolEvent;

// Event type constants (avoid .to_string() allocations)
const EVENT_TYPE_EVENT_START: &str = "event.start";
const EVENT_TYPE_EVENT_END: &str = "event.end";
const EVENT_TYPE_TOOL_REQUEST: &str = "tool.request";
const EVENT_TYPE_TOOL_RESULT: &str = "tool.result";
const EVENT_TYPE_ASSISTANT_OUTPUT: &str = "assistant.output";
const EVENT_TYPE_ASSISTANT_REASONING: &str = "assistant.reasoning";

// Special string markers
const NO_CONTENT: &str = "(no content)";
const EMPTY_MESSAGE: &str = "{}";

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
    fn current_ts(&mut self) -> &str {
        const REFRESH_INTERVAL_MS: u128 = 50;
        if self.last_ts_refresh.elapsed().as_millis() >= REFRESH_INTERVAL_MS {
            self.cached_ts = Local::now().to_rfc3339();
            self.last_ts_refresh = Instant::now();
        }
        &self.cached_ts
    }

    #[inline]
    fn make_event_type(s: &str) -> String {
        String::from(s)
    }

    pub fn parse_value(&mut self, v: &Value) -> Option<ToolEvent> {
        // Extract type once - this is the primary optimization
        // Reduces 20+ v.get("type") calls to just 1
        let type_str = v.get("type").and_then(|x| x.as_str())?;

        let ts = Some(self.current_ts().to_string());

        // === HOT PATH: Gemini tool_use/tool_result (most common) ===
        match type_str {
            "tool_use" => {
                // Fast path: extract all fields in one pass
                let tool_name = v.get("tool_name")?.as_str()?;
                let tool_id = v.get("tool_id")?.as_str()?;
                let args = v.get("parameters").cloned().unwrap_or(Value::Null);

                // Cache tool name for result lookup
                self.pending_tool_name_by_id
                    .insert(tool_id.to_string(), tool_name.to_string());

                let event_ts = v
                    .get("timestamp")
                    .and_then(|x| x.as_str())
                    .map(|x| x.to_string());

                return Some(ToolEvent {
                    v: 1,
                    event_type: Self::make_event_type(EVENT_TYPE_TOOL_REQUEST),
                    ts: event_ts.or(ts),
                    run_id: None,
                    id: Some(tool_id.to_string()),
                    tool: Some(tool_name.to_string()),
                    action: None,
                    args,
                    ok: None,
                    output: None,
                    error: None,
                    rationale: None,
                });
            }
            "tool_result" => {
                // Fast path: extract all fields in one pass
                let tool_id = v.get("tool_id")?.as_str()?;
                let ok = match v.get("status").and_then(|x| x.as_str()) {
                    Some("success") => Some(true),
                    Some("error") => Some(false),
                    _ => None,
                };
                let output = v.get("output").cloned();

                let event_ts = v
                    .get("timestamp")
                    .and_then(|x| x.as_str())
                    .map(|x| x.to_string());

                // Look up tool name from cache
                let tool = self.pending_tool_name_by_id.get(tool_id).cloned();

                return Some(ToolEvent {
                    v: 1,
                    event_type: Self::make_event_type(EVENT_TYPE_TOOL_RESULT),
                    ts: event_ts.or(ts),
                    run_id: None,
                    id: Some(tool_id.to_string()),
                    tool,
                    action: None,
                    args: Value::Null,
                    ok,
                    output,
                    error: None,
                    rationale: None,
                });
            }
            _ => {}
        }

        // === COLD PATH: Other event types ===

        // Claude: system with subtype
        if type_str == "system" && v.get("subtype").is_some() {
            let session_id = v
                .get("session_id")
                .and_then(|x| x.as_str())
                .map(|x| x.to_string());
            let subtype = v.get("subtype").and_then(|x| x.as_str()).unwrap_or("");
            return Some(ToolEvent {
                v: 1,
                event_type: Self::make_event_type(EVENT_TYPE_EVENT_START),
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

        // Claude: result with subtype
        if type_str == "result" && v.get("subtype").is_some() {
            let subtype = v.get("subtype").and_then(|x| x.as_str()).unwrap_or("");
            let result = v.get("result").cloned().unwrap_or(Value::Null);
            let is_error = v.get("is_error").and_then(|x| x.as_bool()).unwrap_or(false);
            return Some(ToolEvent {
                v: 1,
                event_type: Self::make_event_type(EVENT_TYPE_EVENT_END),
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

        // Claude: assistant with nested content
        if type_str == "assistant" {
            if let Some(assistant_message) = v.get("message").and_then(|c| c.as_object()) {
                let default_message = Value::String(EMPTY_MESSAGE.to_string());
                let message = assistant_message.get("message").unwrap_or(&default_message);
                let items = assistant_message
                    .get("content")
                    .and_then(|c| c.as_array())?;

                for item in items {
                    let item_type = item.get("type")?.as_str()?;

                    match item_type {
                        "tool_use" => {
                            let id = item.get("id")?.as_str()?.to_string();
                            let tool = item.get("name")?.as_str()?.to_string();
                            let args = item.get("input").cloned().unwrap_or(Value::Null);
                            let name = item.get("name")?.as_str();

                            self.pending_tool_name_by_id
                                .insert(id.clone(), tool.clone());

                            return Some(ToolEvent {
                                v: 1,
                                event_type: Self::make_event_type(EVENT_TYPE_TOOL_REQUEST),
                                ts: ts.clone(),
                                run_id: None,
                                id: Some(id),
                                tool: Some(tool),
                                action: name.map(|x| x.to_string()),
                                args: args.clone(),
                                ok: None,
                                output: Some(args),
                                error: None,
                                rationale: None,
                            });
                        }
                        "text" | "thinking" => {
                            let field = if item_type == "thinking" {
                                "thinking"
                            } else {
                                "text"
                            };
                            let mut content = item
                                .get(field)
                                .and_then(|x| x.as_str())
                                .map(|x| x.to_string())
                                .unwrap_or_default();
                            if content == NO_CONTENT {
                                content = String::new();
                            }
                            return Some(ToolEvent {
                                v: 1,
                                event_type: if item_type == "thinking" {
                                    Self::make_event_type(EVENT_TYPE_ASSISTANT_REASONING)
                                } else {
                                    Self::make_event_type(EVENT_TYPE_ASSISTANT_OUTPUT)
                                },
                                ts: ts.clone(),
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
            }
        }

        // Claude: user with nested content
        if type_str == "user" {
            if let Some(user_message) = v.get("message").and_then(|c| c.as_object()) {
                let default_message = Value::String(EMPTY_MESSAGE.to_string());
                let message = user_message.get("message").unwrap_or(&default_message);
                let items = user_message.get("content").and_then(|c| c.as_array())?;

                for item in items {
                    if item.get("type")?.as_str()? == "tool_result" {
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
                            event_type: Self::make_event_type(EVENT_TYPE_TOOL_RESULT),
                            ts: ts.clone(),
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
                        let mut content = item
                            .get("text")
                            .and_then(|x| x.as_str())
                            .map(|x| x.to_string())
                            .unwrap_or_default();
                        if content == NO_CONTENT {
                            content = String::new();
                        }
                        return Some(ToolEvent {
                            v: 1,
                            event_type: Self::make_event_type(EVENT_TYPE_ASSISTANT_OUTPUT),
                            ts: ts.clone(),
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

        // Gemini: init
        if type_str == "init" {
            return Some(ToolEvent {
                v: 1,
                event_type: Self::make_event_type(EVENT_TYPE_EVENT_START),
                ts,
                run_id: None,
                id: None,
                tool: None,
                action: Some(type_str.to_string()),
                args: Value::Null,
                ok: None,
                output: Some(Value::String(v.to_string())),
                error: None,
                rationale: None,
            });
        }

        // Gemini: result (without subtype - different from Claude's result)
        if type_str == "result" && v.get("subtype").is_some() {
            let status = v
                .get("status")
                .and_then(|x| x.as_str())
                .map(|s| s == "success");
            let stats = v.get("stats").cloned().unwrap_or(Value::Null);

            return Some(ToolEvent {
                v: 1,
                event_type: Self::make_event_type(EVENT_TYPE_EVENT_END),
                ts,
                run_id: None,
                id: None,
                tool: None,
                action: Some(type_str.to_string()),
                args: Value::Null,
                ok: status,
                output: Some(stats),
                error: None,
                rationale: None,
            });
        }

        // Gemini: message
        if type_str == "message" {
            let role = v.get("role").and_then(|x| x.as_str()).unwrap_or("");

            if role == "assistant" {
                let content = v.get("content").cloned().unwrap_or(Value::Null);

                return Some(ToolEvent {
                    v: 1,
                    event_type: Self::make_event_type(EVENT_TYPE_ASSISTANT_OUTPUT),
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

        // Turn events
        match type_str {
            "turn.started" => {
                return Some(ToolEvent {
                    v: 1,
                    event_type: Self::make_event_type(EVENT_TYPE_EVENT_START),
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
                })
            }
            "turn.completed" => {
                let usage = v.get("usage").cloned().unwrap_or(Value::Null);
                return Some(ToolEvent {
                    v: 1,
                    event_type: Self::make_event_type(EVENT_TYPE_EVENT_END),
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
            _ => {}
        }

        // === CODEX FORMAT: item field ===
        if let Some(item) = v.get("item") {
            return self.parse_codex_item(v, item, ts, type_str);
        }

        None
    }

    /// Parse Codex format items (cold path)
    fn parse_codex_item(
        &mut self,
        _v: &Value,
        item: &Value,
        ts: Option<String>,
        type_str: &str,
    ) -> Option<ToolEvent> {
        let item_type = item.get("type")?.as_str()?;

        match item_type {
            "mcp_tool_call" => {
                let id = item.get("id")?.as_str()?.to_string();
                let tool = item.get("tool")?.as_str()?.to_string();
                let server = item.get("server")?.as_str()?.to_string();
                let args = item.get("arguments").cloned().unwrap_or(Value::Null);

                match type_str {
                    "item.started" => {
                        return Some(ToolEvent {
                            v: 1,
                            event_type: Self::make_event_type(EVENT_TYPE_TOOL_REQUEST),
                            ts,
                            run_id: None,
                            id: Some(id),
                            tool: Some(server),
                            action: Some(tool),
                            args,
                            ok: None,
                            output: None,
                            error: None,
                            rationale: None,
                        });
                    }
                    "item.completed" => {
                        let status = item.get("status").and_then(|x| x.as_str()).unwrap_or("");
                        let ok = match status {
                            "completed" => Some(true),
                            "failed" => Some(false),
                            _ => None,
                        };

                        let output = item.get("result").cloned();
                        let error = item
                            .get("error")
                            .and_then(|x| x.as_str())
                            .map(|x| x.to_string());

                        return Some(ToolEvent {
                            v: 1,
                            event_type: Self::make_event_type(EVENT_TYPE_TOOL_RESULT),
                            ts,
                            run_id: None,
                            id: Some(id),
                            tool: Some(server),
                            action: Some(tool),
                            args,
                            ok,
                            output,
                            error,
                            rationale: None,
                        });
                    }
                    _ => {}
                }
            }
            "agent_message" => {
                if type_str == "item.completed" {
                    let id = item.get("id")?.as_str()?.to_string();
                    let text = item
                        .get("text")
                        .and_then(|x| x.as_str())
                        .unwrap_or_default()
                        .to_string();

                    return Some(ToolEvent {
                        v: 1,
                        event_type: Self::make_event_type(EVENT_TYPE_ASSISTANT_OUTPUT),
                        ts,
                        run_id: None,
                        id: Some(id),
                        tool: None,
                        action: None,
                        args: Value::Null,
                        ok: None,
                        output: Some(Value::String(text)),
                        error: None,
                        rationale: None,
                    });
                }
            }
            "reasoning" => {
                if type_str == "item.completed" {
                    let id = item.get("id")?.as_str()?.to_string();
                    let text = item
                        .get("text")
                        .and_then(|x| x.as_str())
                        .unwrap_or_default()
                        .to_string();

                    return Some(ToolEvent {
                        v: 1,
                        event_type: Self::make_event_type(EVENT_TYPE_ASSISTANT_REASONING),
                        ts,
                        run_id: None,
                        id: Some(id),
                        tool: None,
                        action: None,
                        args: Value::Null,
                        ok: None,
                        output: Some(Value::String(text)),
                        error: None,
                        rationale: None,
                    });
                }
            }
            "command_execution" => {
                let id = item.get("id")?.as_str()?.to_string();
                let command = item.get("command").cloned().unwrap_or(Value::Null);

                match type_str {
                    "item.started" => {
                        return Some(ToolEvent {
                            v: 1,
                            event_type: Self::make_event_type(EVENT_TYPE_TOOL_REQUEST),
                            ts: ts.clone(),
                            run_id: None,
                            id: Some(id.clone()),
                            tool: Some("command_execution".to_string()),
                            action: Some("exec".to_string()),
                            args: serde_json::json!({ "command": command }),
                            ok: None,
                            output: None,
                            error: None,
                            rationale: None,
                        });
                    }
                    "item.completed" => {
                        let exit_code = item.get("exit_code")?.as_i64()?;
                        let ok = exit_code == 0;
                        let output = item.get("aggregated_output").cloned();
                        let status = item.get("status")?.as_str()?.to_string();
                        let error = if status == "failed" {
                            Some("command_execution_failed".to_string())
                        } else {
                            None
                        };

                        return Some(ToolEvent {
                            v: 1,
                            event_type: Self::make_event_type(EVENT_TYPE_TOOL_RESULT),
                            ts,
                            run_id: None,
                            id: Some(id),
                            tool: Some("command_execution".to_string()),
                            action: Some("exec".to_string()),
                            args: serde_json::json!({ "command": command }),
                            ok: Some(ok),
                            output: output.or_else(|| {
                                Some(
                                    serde_json::json!({ "exit_code": exit_code, "status": status }),
                                )
                            }),
                            error,
                            rationale: None,
                        });
                    }
                    _ => {}
                }
            }
            _ => {}
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
