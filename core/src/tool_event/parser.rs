pub use crate::tool_event::ToolEvent;

pub trait ToolEventParser: Send {
    fn parse_line(&mut self, line: &str) -> Option<ToolEvent>;
    fn format_line(&self, ev: &ToolEvent) -> String;
}

pub struct PrefixedJsonlParser {
    prefix: &'static str,
}

impl PrefixedJsonlParser {
    pub fn new(prefix: &'static str) -> Self {
        Self { prefix }
    }
}

impl ToolEventParser for PrefixedJsonlParser {
    fn parse_line(&mut self, line: &str) -> Option<ToolEvent> {
        let s = line.trim();
        if !s.starts_with(self.prefix) {
            return None;
        }
        let json_part = s[self.prefix.len()..].trim();
        if json_part.is_empty() {
            return None;
        }
        serde_json::from_str::<ToolEvent>(json_part).ok()
    }

    fn format_line(&self, ev: &ToolEvent) -> String {
        let json = serde_json::to_string(ev).unwrap_or_else(|_| "{}".to_string());
        format!("{} {}", self.prefix, json)
    }
}

pub struct CompositeToolEventParser {
    prefixed: PrefixedJsonlParser,
    stream_json: crate::tool_event::StreamJsonToolEventParser,
}

impl CompositeToolEventParser {
    pub fn new(prefix: &'static str) -> Self {
        Self {
            prefixed: PrefixedJsonlParser::new(prefix),
            stream_json: crate::tool_event::StreamJsonToolEventParser::new(),
        }
    }
}

impl ToolEventParser for CompositeToolEventParser {
    fn parse_line(&mut self, line: &str) -> Option<ToolEvent> {
        self.prefixed
            .parse_line(line)
            .or_else(|| self.stream_json.parse_line(line))
    }

    fn format_line(&self, ev: &ToolEvent) -> String {
        self.prefixed.format_line(ev)
    }
}
