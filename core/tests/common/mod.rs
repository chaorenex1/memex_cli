use memex_core::tool_event::{MultiToolEventLineParser, ToolEvent, TOOL_EVENT_PREFIX};

pub fn parse_events_from_str(input: &str) -> Vec<ToolEvent> {
    let mut parser = MultiToolEventLineParser::new(TOOL_EVENT_PREFIX);
    input
        .lines()
        .filter_map(|line| parser.parse_line(line))
        .collect()
}

pub fn first_tool_request_with_id_and_tool(events: &[ToolEvent]) -> Option<(String, String)> {
    events
        .iter()
        .find(|ev| ev.event_type == "tool.request" && ev.id.is_some() && ev.tool.is_some())
        .and_then(|ev| Some((ev.id.clone()?, ev.tool.clone()?)))
}

pub fn find_tool_result_by_id<'a>(events: &'a [ToolEvent], id: &str) -> Option<&'a ToolEvent> {
    events
        .iter()
        .find(|ev| ev.event_type == "tool.result" && ev.id.as_deref() == Some(id))
}
