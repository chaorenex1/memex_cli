use crate::tool_event::model::{ToolEvent, TOOL_EVENT_PREFIX};
use crate::tool_event::{PrefixedJsonlParser, StreamJsonToolEventParser, ToolEventParser};

/// Stateful, best-effort parser for mixed stdout/stderr logs.
///
/// Supported inputs (in this order):
/// 1) Prefixed JSONL: `@@MEM_TOOL_EVENT@@ { ...ToolEvent... }`
/// 2) External stream-json formats (gemini/codex/claude) via `StreamJsonToolEventParser`
/// 3) Raw ToolEvent JSON (must match `ToolEvent` schema)
///
/// Use this when you need to parse a whole log sequentially (e.g. replay), where
/// some formats require cross-line correlation (like gemini tool_result without tool_name).
pub struct MultiToolEventLineParser {
    prefixed: PrefixedJsonlParser,
    stream_json: StreamJsonToolEventParser,
}

impl MultiToolEventLineParser {
    pub fn new(prefix: &'static str) -> Self {
        Self {
            prefixed: PrefixedJsonlParser::new(prefix),
            stream_json: StreamJsonToolEventParser::new(),
        }
    }

    pub fn parse_line(&mut self, line: &str) -> Option<ToolEvent> {
        self.prefixed
            .parse_line(line)
            .or_else(|| self.stream_json.parse_line(line))
            .or_else(|| serde_json::from_str::<ToolEvent>(line.trim()).ok())
    }
}

pub fn parse_tool_event_line(line: &str) -> Option<ToolEvent> {
    let s = line.trim();
    if !s.starts_with(TOOL_EVENT_PREFIX) {
        // Backward-compatible behavior: this helper only recognizes the stable ToolEvent schema.
        // For external stream-json formats, use `MultiToolEventLineParser` instead.
        return serde_json::from_str::<ToolEvent>(s).ok();
    }
    let json_part = s[TOOL_EVENT_PREFIX.len()..].trim();
    if json_part.is_empty() {
        return None;
    }
    serde_json::from_str::<ToolEvent>(json_part).ok()
}

pub fn format_tool_event_line(ev: &ToolEvent) -> String {
    let json = serde_json::to_string(ev).unwrap_or_else(|_| "{}".to_string());
    format!("{TOOL_EVENT_PREFIX} {json}")
}
