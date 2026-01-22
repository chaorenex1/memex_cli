use std::collections::HashMap;
use std::time::Instant;

use chrono::Local;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use memex_core::api::ToolEvent;

/// Tagged enum for stream-json events - enables automatic deserialization
/// This eliminates manual field extraction overhead
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
enum StreamEventType {
    /// Claude/Gemini: System initialization
    #[serde(rename = "init")]
    Init {
        timestamp: Option<String>,
        session_id: Option<String>,
        model: Option<String>,
    },

    /// Gemini: Tool use event
    #[serde(rename = "tool_use")]
    ToolUse {
        timestamp: Option<String>,
        tool_name: String,
        tool_id: String,
        parameters: Value,
    },

    /// Gemini: Tool result event
    #[serde(rename = "tool_result")]
    ToolResult {
        timestamp: Option<String>,
        tool_id: String,
        status: String,
        output: Option<Value>,
    },

    /// Claude: Assistant message
    Assistant {
        message: AssistantMessage,
    },

    /// Claude: User message
    User {
        message: UserMessage,
    },

    /// Gemini: Generic message
    Message {
        role: String,
        content: Value,
    },

    /// Codex: Turn events
    #[serde(rename = "turn.started")]
    TurnStarted,

    #[serde(rename = "turn.completed")]
    TurnCompleted {
        usage: Option<Value>,
    },

    /// Codex: Item events with nested item
    #[serde(rename = "item.started")]
    ItemStarted {
        item: ItemData,
    },

    #[serde(rename = "item.completed")]
    ItemCompleted {
        item: ItemData,
    },

    /// Fallback for unknown types
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct AssistantMessage {
    content: Vec<ContentItem>,
    #[serde(default)]
    message: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct UserMessage {
    content: Vec<ContentItem>,
    #[serde(default)]
    message: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
enum ContentItem {
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: Option<Value>,
    },
    Text {
        text: String,
    },
    Thinking {
        thinking: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
enum ItemData {
    McpToolCall {
        id: Option<String>,
        tool: Option<String>,
        server: Option<String>,
        arguments: Option<Value>,
        status: Option<String>,
        result: Option<Value>,
        error: Option<String>,
    },
    AgentMessage {
        id: Option<String>,
        text: String,
    },
    Reasoning {
        id: Option<String>,
        text: String,
    },
    CommandExecution {
        id: Option<String>,
        command: Value,
        exit_code: Option<i64>,
        aggregated_output: Option<Value>,
        status: Option<String>,
    },
    #[serde(other)]
    Other,
}

/// Optimized parser using tagged enum for automatic deserialization
pub struct StreamJsonToolEventParserV2 {
    pending_tool_name_by_id: HashMap<String, String>,
    cached_ts: String,
    last_ts_refresh: Instant,
}

impl Default for StreamJsonToolEventParserV2 {
    fn default() -> Self {
        Self {
            pending_tool_name_by_id: HashMap::new(),
            cached_ts: Local::now().to_rfc3339(),
            last_ts_refresh: Instant::now(),
        }
    }
}

impl StreamJsonToolEventParserV2 {
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

    /// Optimized parse_value using automatic deserialization
    pub fn parse_value(&mut self, v: &Value) -> Option<ToolEvent> {
        // Fast path: try automatic deserialization first
        let event: StreamEventType = serde_json::from_value(v.clone()).ok()?;
        let ts = Some(self.current_ts().to_string());

        match event {
            StreamEventType::Init { session_id, .. } => Some(ToolEvent {
                v: 1,
                event_type: "event.start".to_string(),
                ts,
                run_id: session_id,
                id: None,
                tool: None,
                action: Some("init".to_string()),
                args: Value::Null,
                ok: None,
                output: Some(Value::String(v.to_string())),
                error: None,
                rationale: None,
            }),

            StreamEventType::ToolUse {
                timestamp,
                tool_name,
                tool_id,
                parameters,
            } => {
                self.pending_tool_name_by_id.insert(tool_id.clone(), tool_name.clone());
                Some(ToolEvent {
                    v: 1,
                    event_type: "tool.request".to_string(),
                    ts: timestamp.or(ts),
                    run_id: None,
                    id: Some(tool_id),
                    tool: Some(tool_name),
                    action: None,
                    args: parameters,
                    ok: None,
                    output: None,
                    error: None,
                    rationale: None,
                })
            }

            StreamEventType::ToolResult {
                timestamp,
                tool_id,
                status,
                output,
            } => {
                let ok = match status.as_str() {
                    "success" => Some(true),
                    "error" => Some(false),
                    _ => None,
                };
                let tool = self.pending_tool_name_by_id.get(&tool_id).cloned();
                Some(ToolEvent {
                    v: 1,
                    event_type: "tool.result".to_string(),
                    ts: timestamp.or(ts),
                    run_id: None,
                    id: Some(tool_id),
                    tool,
                    action: None,
                    args: Value::Null,
                    ok,
                    output,
                    error: None,
                    rationale: None,
                })
            }

            StreamEventType::Assistant { message } => {
                self.handle_assistant_message(&message, ts)
            }

            StreamEventType::User { message } => {
                self.handle_user_message(&message, ts)
            }

            StreamEventType::Message { role, content } => {
                if role == "assistant" {
                    Some(ToolEvent {
                        v: 1,
                        event_type: "assistant.output".to_string(),
                        ts,
                        run_id: None,
                        id: None,
                        tool: None,
                        action: Some(role),
                        args: Value::Null,
                        ok: None,
                        output: Some(content),
                        error: None,
                        rationale: None,
                    })
                } else {
                    None
                }
            }

            StreamEventType::TurnStarted => Some(ToolEvent {
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

            StreamEventType::TurnCompleted { usage } => Some(ToolEvent {
                v: 1,
                event_type: "event.end".to_string(),
                ts,
                run_id: None,
                id: None,
                tool: None,
                action: None,
                args: Value::Null,
                ok: Some(true),
                output: usage,
                error: None,
                rationale: None,
            }),

            StreamEventType::ItemStarted { ref item } | StreamEventType::ItemCompleted { ref item } => {
                let is_started = matches!(&event, StreamEventType::ItemStarted { .. });
                self.handle_item_event(item, ts, is_started)
            }

            StreamEventType::Unknown => None,
        }
    }

    fn handle_assistant_message(
        &mut self,
        message: &AssistantMessage,
        ts: Option<String>,
    ) -> Option<ToolEvent> {
        for item in &message.content {
            match item {
                ContentItem::ToolUse { id, name, input } => {
                    self.pending_tool_name_by_id.insert(id.clone(), name.clone());
                    return Some(ToolEvent {
                        v: 1,
                        event_type: "tool.request".to_string(),
                        ts,
                        run_id: None,
                        id: Some(id.clone()),
                        tool: Some(name.clone()),
                        action: Some(name.clone()),
                        args: input.clone(),
                        ok: None,
                        output: Some(input.clone()),
                        error: None,
                        rationale: None,
                    });
                }
                ContentItem::Text { text } => {
                    let content = if text == "(no content)" { "" } else { text };
                    return Some(ToolEvent {
                        v: 1,
                        event_type: "assistant.output".to_string(),
                        ts,
                        run_id: None,
                        id: None,
                        tool: None,
                        action: Some(message.message.to_string()),
                        args: Value::Null,
                        ok: None,
                        output: Some(Value::String(content.to_string())),
                        error: None,
                        rationale: None,
                    });
                }
                ContentItem::Thinking { thinking } => {
                    let content = if thinking == "(no content)" { "" } else { thinking };
                    return Some(ToolEvent {
                        v: 1,
                        event_type: "assistant.reasoning".to_string(),
                        ts,
                        run_id: None,
                        id: None,
                        tool: None,
                        action: Some(message.message.to_string()),
                        args: Value::Null,
                        ok: None,
                        output: Some(Value::String(content.to_string())),
                        error: None,
                        rationale: None,
                    });
                }
                _ => {}
            }
        }
        None
    }

    fn handle_user_message(
        &mut self,
        message: &UserMessage,
        ts: Option<String>,
    ) -> Option<ToolEvent> {
        for item in &message.content {
            if let ContentItem::ToolResult { tool_use_id, content } = item {
                let tool = self.pending_tool_name_by_id.get(tool_use_id).cloned();
                let ok = content.as_ref().map(|_| true);
                return Some(ToolEvent {
                    v: 1,
                    event_type: "tool.result".to_string(),
                    ts,
                    run_id: None,
                    id: Some(tool_use_id.clone()),
                    tool,
                    action: Some(message.message.to_string()),
                    args: Value::Null,
                    ok,
                    output: content.clone(),
                    error: None,
                    rationale: None,
                });
            }
        }
        None
    }

    fn handle_item_event(
        &mut self,
        item: &ItemData,
        ts: Option<String>,
        is_started: bool,
    ) -> Option<ToolEvent> {
        match item {
            ItemData::McpToolCall {
                id,
                tool,
                server,
                arguments,
                status,
                result,
                error,
            } => {
                if is_started {
                    Some(ToolEvent {
                        v: 1,
                        event_type: "tool.request".to_string(),
                        ts,
                        run_id: None,
                        id: id.clone(),
                        tool: server.clone(),
                        action: tool.clone(),
                        args: arguments.clone().unwrap_or(Value::Null),
                        ok: None,
                        output: None,
                        error: None,
                        rationale: None,
                    })
                } else {
                    let ok = status.as_ref().and_then(|s| match s.as_str() {
                        "completed" => Some(true),
                        "failed" => Some(false),
                        _ => None,
                    });
                    Some(ToolEvent {
                        v: 1,
                        event_type: "tool.result".to_string(),
                        ts,
                        run_id: None,
                        id: id.clone(),
                        tool: server.clone(),
                        action: tool.clone(),
                        args: arguments.clone().unwrap_or(Value::Null),
                        ok,
                        output: result.clone(),
                        error: error.clone(),
                        rationale: None,
                    })
                }
            }

            ItemData::AgentMessage { id, text } => Some(ToolEvent {
                v: 1,
                event_type: "assistant.output".to_string(),
                ts,
                run_id: None,
                id: id.clone(),
                tool: None,
                action: None,
                args: Value::Null,
                ok: None,
                output: Some(Value::String(text.clone())),
                error: None,
                rationale: None,
            }),

            ItemData::Reasoning { id, text } => Some(ToolEvent {
                v: 1,
                event_type: "assistant.reasoning".to_string(),
                ts,
                run_id: None,
                id: id.clone(),
                tool: None,
                action: None,
                args: Value::Null,
                ok: None,
                output: Some(Value::String(text.clone())),
                error: None,
                rationale: None,
            }),

            ItemData::CommandExecution {
                id,
                command,
                exit_code,
                aggregated_output,
                status,
            } => {
                if is_started {
                    Some(ToolEvent {
                        v: 1,
                        event_type: "tool.request".to_string(),
                        ts,
                        run_id: None,
                        id: id.clone(),
                        tool: Some("command_execution".to_string()),
                        action: Some("exec".to_string()),
                        args: serde_json::json!({ "command": command }),
                        ok: None,
                        output: None,
                        error: None,
                        rationale: None,
                    })
                } else {
                    let ok = exit_code.map(|c| c == 0);
                    let error = if status.as_deref() == Some("failed") {
                        Some("command_execution_failed".to_string())
                    } else {
                        None
                    };
                    Some(ToolEvent {
                        v: 1,
                        event_type: "tool.result".to_string(),
                        ts,
                        run_id: None,
                        id: id.clone(),
                        tool: Some("command_execution".to_string()),
                        action: Some("exec".to_string()),
                        args: serde_json::json!({ "command": command }),
                        ok,
                        output: aggregated_output.clone().or_else(|| {
                            Some(serde_json::json!({
                                "exit_code": exit_code,
                                "status": status
                            }))
                        }),
                        error,
                        rationale: None,
                    })
                }
            }

            ItemData::Other => None,
        }
    }

    #[allow(dead_code)]
    pub fn parse_line(&mut self, line: &str) -> Option<ToolEvent> {
        let s = line.trim();
        if !(s.starts_with('{') && s.ends_with('}')) {
            return None;
        }

        let v: Value = serde_json::from_str(s).ok()?;
        self.parse_value(&v)
    }
}
