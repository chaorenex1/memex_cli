mod common;

use common::{find_tool_result_by_id, first_tool_request_with_id_and_tool, parse_events_from_str};

#[test]
fn parses_real_claude_stream_json_log() {
    let input = include_str!("../../docs/claude_out.txt");
    let events = parse_events_from_str(input);

    assert!(
        events.iter().any(|e| e.event_type == "tool.request"),
        "expected at least one tool.request in claude log"
    );
    assert!(
        events.iter().any(|e| e.event_type == "tool.result"),
        "expected at least one tool.result in claude log"
    );

    // Claude tool_use/tool_result are wrapped in envelopes; spot-check basic fields and id round-trip.
    let (id, _tool) = first_tool_request_with_id_and_tool(&events)
        .expect("expected a tool.request with id+tool in claude log");

    let _result = find_tool_result_by_id(&events, &id)
        .expect("expected a matching tool.result for the first tool.request id");
}
