//! 对比原始实现 vs Tagged Enum V2实现
//!
//! 测试自动反序列化是否能减少手动字段提取开销

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use serde_json::Value;

// V2实现（需要手动导入）
mod stream_json_v2 {
    include!("../src/tool_event/stream_json_v2.rs");
}

/// 对比单次解析性能
fn bench_single_parse_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("single_parse_comparison");

    let tool_use = r#"{"type":"tool_use","timestamp":"2025-12-26T12:48:36.765Z","tool_name":"run_shell_command","tool_id":"run_shell_command-1766753316765-e8db","parameters":{"command":"echo hi"}}"#;
    let tool_result = r#"{"type":"tool_result","timestamp":"2025-12-26T12:48:38.811Z","tool_id":"run_shell_command-1766753316765-e8db","status":"success","output":""}"#;

    // 原始实现
    group.bench_function("original_tool_use", |b| {
        b.iter(|| {
            let mut parser = memex_core::api::StreamJsonToolEventParser::new();
            let v: Value = serde_json::from_str(black_box(tool_use)).unwrap();
            parser.parse_value(black_box(&v))
        })
    });

    // V2实现
    group.bench_function("v2_tool_use", |b| {
        b.iter(|| {
            let mut parser = stream_json_v2::StreamJsonToolEventParserV2::new();
            let v: Value = serde_json::from_str(black_box(tool_use)).unwrap();
            parser.parse_value(black_box(&v))
        })
    });

    // 原始实现
    group.bench_function("original_tool_result", |b| {
        b.iter(|| {
            let mut parser = memex_core::api::StreamJsonToolEventParser::new();
            let v: Value = serde_json::from_str(black_box(tool_result)).unwrap();
            parser.parse_value(black_box(&v))
        })
    });

    // V2实现
    group.bench_function("v2_tool_result", |b| {
        b.iter(|| {
            let mut parser = stream_json_v2::StreamJsonToolEventParserV2::new();
            let v: Value = serde_json::from_str(black_box(tool_result)).unwrap();
            parser.parse_value(black_box(&v))
        })
    });

    group.finish();
}

/// 对比批量处理性能
fn bench_batch_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_comparison");

    let tool_use = r#"{"type":"tool_use","timestamp":"2025-12-26T12:48:36.765Z","tool_name":"run_shell_command","tool_id":"run_shell_command-1766753316765-e8db","parameters":{"command":"echo hi"}}"#;
    let tool_result = r#"{"type":"tool_result","timestamp":"2025-12-26T12:48:38.811Z","tool_id":"run_shell_command-1766753316765-e8db","status":"success","output":""}"#;

    for count in [100, 500, 1000].iter() {
        group.bench_with_input(BenchmarkId::new("original", count), count, |b, &count| {
            b.iter(|| {
                let mut parser = memex_core::api::StreamJsonToolEventParser::new();
                for i in 0..count {
                    let line = if i % 2 == 0 { tool_use } else { tool_result };
                    let v: Value = serde_json::from_str(black_box(line)).unwrap();
                    parser.parse_value(black_box(&v));
                }
            })
        });

        group.bench_with_input(BenchmarkId::new("v2", count), count, |b, &count| {
            b.iter(|| {
                let mut parser = stream_json_v2::StreamJsonToolEventParserV2::new();
                for i in 0..count {
                    let line = if i % 2 == 0 { tool_use } else { tool_result };
                    let v: Value = serde_json::from_str(black_box(line)).unwrap();
                    parser.parse_value(black_box(&v));
                }
            })
        });
    }

    group.finish();
}

/// 对比不同事件类型的性能
fn bench_different_types(c: &mut Criterion) {
    let mut group = c.benchmark_group("different_types");

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
    ];

    for (name, json) in test_cases.iter() {
        group.bench_function(format!("original_{}", name), |b| {
            b.iter(|| {
                let mut parser = memex_core::api::StreamJsonToolEventParser::new();
                let v: Value = serde_json::from_str(black_box(json)).unwrap();
                parser.parse_value(black_box(&v))
            })
        });

        group.bench_function(format!("v2_{}", name), |b| {
            b.iter(|| {
                let mut parser = stream_json_v2::StreamJsonToolEventParserV2::new();
                let v: Value = serde_json::from_str(black_box(json)).unwrap();
                parser.parse_value(black_box(&v))
            })
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_single_parse_comparison,
    bench_batch_comparison,
    bench_different_types
);
criterion_main!(benches);
