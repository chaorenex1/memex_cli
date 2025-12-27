mod common;

use common::{find_tool_result_by_id, first_tool_request_with_id_and_tool, parse_events_from_str};

#[test]
fn parses_real_codex_stream_json_log() {
    let input = include_str!("../../docs/codex_out.txt");
    let events = parse_events_from_str(input);

    assert!(
        events.iter().any(|e| e.event_type == "tool.request"),
        "expected at least one tool.request in codex log"
    );
    assert!(
        events.iter().any(|e| e.event_type == "tool.result"),
        "expected at least one tool.result in codex log"
    );

    // Codex mcp_tool_call uses item.started/item.completed; ensure we can round-trip id.
    let (id, tool) = first_tool_request_with_id_and_tool(&events)
        .expect("expected at least one tool.request with id+tool");

    let result = find_tool_result_by_id(&events, &id)
        .expect("expected a matching tool.result for the first tool.request");

    assert_eq!(result.tool.as_deref(), Some(tool.as_str()));
}
