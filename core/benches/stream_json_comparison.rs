//! 对比优化前后的stream_json性能
//!
//! 由于优化版本尚未集成，本测试作为基准参考

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use memex_core::tool_event::StreamJsonToolEventParser;

/// 测试单次解析性能（基准）
fn bench_single_parse_baseline(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_baseline");

    let tool_use = r#"{"type":"tool_use","timestamp":"2025-12-26T12:48:36.765Z","tool_name":"run_shell_command","tool_id":"run_shell_command-1766753316765-e8db","parameters":{"command":"echo hi"}}"#;
    let tool_result = r#"{"type":"tool_result","timestamp":"2025-12-26T12:48:38.811Z","tool_id":"run_shell_command-1766753316765-e8db","status":"success","output":""}"#;

    group.bench_function("tool_use", |b| {
        b.iter(|| {
            let mut parser = StreamJsonToolEventParser::new();
            parser.parse_line(black_box(tool_use))
        })
    });

    group.bench_function("tool_result", |b| {
        b.iter(|| {
            let mut parser = StreamJsonToolEventParser::new();
            parser.parse_line(black_box(tool_result))
        })
    });

    group.finish();
}

/// 测试批量处理性能（基准）
fn bench_batch_parse_baseline(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_baseline");

    let tool_use = r#"{"type":"tool_use","timestamp":"2025-12-26T12:48:36.765Z","tool_name":"run_shell_command","tool_id":"run_shell_command-1766753316765-e8db","parameters":{"command":"echo hi"}}"#;
    let tool_result = r#"{"type":"tool_result","timestamp":"2025-12-26T12:48:38.811Z","tool_id":"run_shell_command-1766753316765-e8db","status":"success","output":""}"#;

    for count in [100, 500, 1000, 5000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, &count| {
            b.iter(|| {
                let mut parser = StreamJsonToolEventParser::new();
                for i in 0..count {
                    let line = if i % 2 == 0 { tool_use } else { tool_result };
                    parser.parse_line(black_box(line));
                }
            })
        });
    }

    group.finish();
}

/// 测试不同type类型的解析性能
fn bench_different_types_baseline(c: &mut Criterion) {
    let mut group = c.benchmark_group("types_baseline");

    let test_cases = [
        (
            "init",
            r#"{"type":"init","timestamp":"2025-12-26T12:48:29.765Z","session_id":"test-session","model":"auto-gemini-2.5"}"#,
        ),
        (
            "message",
            r#"{"type":"message","role":"assistant","content":"Hello"}"#,
        ),
        ("turn_started", r#"{"type":"turn.started"}"#),
        (
            "turn_completed",
            r#"{"type":"turn.completed","usage":{"input_tokens":100,"output_tokens":50}}"#,
        ),
    ];

    for (name, json) in test_cases.iter() {
        group.bench_function(*name, |b| {
            b.iter(|| {
                let mut parser = StreamJsonToolEventParser::new();
                parser.parse_line(black_box(json))
            })
        });
    }

    group.finish();
}

/// 测试纯文本跳过性能
fn bench_plain_text_skip(c: &mut Criterion) {
    let mut group = c.benchmark_group("text_skip");

    let plain_text = "this is plain text output from command";
    let malformed = r#"{"incomplete json"#;

    group.bench_function("plain_text", |b| {
        b.iter(|| {
            let mut parser = StreamJsonToolEventParser::new();
            parser.parse_line(black_box(plain_text))
        })
    });

    group.bench_function("malformed_json", |b| {
        b.iter(|| {
            let mut parser = StreamJsonToolEventParser::new();
            parser.parse_line(black_box(malformed))
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_single_parse_baseline,
    bench_batch_parse_baseline,
    bench_different_types_baseline,
    bench_plain_text_skip
);
criterion_main!(benches);
