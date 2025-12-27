use std::time::Instant;

use memex_core::tool_event::{CompositeToolEventParser, ToolEventRuntime, TOOL_EVENT_PREFIX};

fn env_usize(name: &str, default_value: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(default_value)
}

/// A lightweight perf-style test for the tool-event parsing hot path.
///
/// Notes:
/// - Marked `#[ignore]` so it doesn't slow normal `cargo test`.
/// - Does not assert on timing (avoids flaky CI). It prints throughput numbers.
///
/// Run:
/// - `cargo test -p memex-core --test perf_tool_event_runtime -- --ignored --nocapture`
/// - Optionally set `MEMEX_PERF_LINES=200000`
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore]
async fn perf_parse_stream_json_lines() {
    let lines = env_usize("MEMEX_PERF_LINES", 50_000);

    // Gemini stream-json carries session_id in an init line.
    let session_id = "dfa4182a-d2da-4dc7-9080-fa2d39bba588";
    let init = format!(
        r#"{{"type":"init","timestamp":"2025-12-26T12:48:29.765Z","session_id":"{}","model":"auto-gemini-2.5"}}"#,
        session_id
    );

    let tool_use = r#"{"type":"tool_use","timestamp":"2025-12-26T12:48:36.765Z","tool_name":"run_shell_command","tool_id":"run_shell_command-1766753316765-e8db","parameters":{"command":"echo hi"}}"#;
    let tool_result = r#"{"type":"tool_result","timestamp":"2025-12-26T12:48:38.811Z","tool_id":"run_shell_command-1766753316765-e8db","status":"success","output":""}"#;

    let parser = CompositeToolEventParser::new(TOOL_EVENT_PREFIX);
    let mut rt = ToolEventRuntime::new(parser, None, Some("local-run-id".to_string()));

    // Prime discovery of session_id.
    rt.observe_line(&init).await;

    let start = Instant::now();
    let mut events = 0usize;

    for i in 0..lines {
        let line = if i % 2 == 0 { tool_use } else { tool_result };
        if rt.observe_line(line).await.is_some() {
            events += 1;
        }
    }

    let elapsed = start.elapsed();
    let secs = elapsed.as_secs_f64().max(1e-9);
    let lines_per_sec = (lines as f64) / secs;
    let events_per_sec = (events as f64) / secs;

    // Stable correctness checks (not timing-based).
    assert_eq!(events, lines);
    let evs = rt.take_events();
    assert_eq!(evs.len(), lines);
    assert!(evs.iter().all(|e| e.run_id.as_deref() == Some(session_id)));

    eprintln!(
        "perf_parse_stream_json_lines: lines={} events={} elapsed={:?} lines/s={:.0} events/s={:.0}",
        lines, events, elapsed, lines_per_sec, events_per_sec
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore]
async fn perf_skip_plain_text_lines() {
    let lines = env_usize("MEMEX_PERF_LINES", 200_000);
    let plain = "this is not json and should be skipped quickly";

    let parser = CompositeToolEventParser::new(TOOL_EVENT_PREFIX);
    let mut rt = ToolEventRuntime::new(parser, None, Some("local-run-id".to_string()));

    let start = Instant::now();
    let mut events = 0usize;

    for _ in 0..lines {
        if rt.observe_line(plain).await.is_some() {
            events += 1;
        }
    }

    let elapsed = start.elapsed();
    let secs = elapsed.as_secs_f64().max(1e-9);
    let lines_per_sec = (lines as f64) / secs;

    assert_eq!(events, 0);

    eprintln!(
        "perf_skip_plain_text_lines: lines={} elapsed={:?} lines/s={:.0}",
        lines, elapsed, lines_per_sec
    );
}
