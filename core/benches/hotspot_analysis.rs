//! 细粒度热点分析基准测试
//!
//! 针对已识别的CPU热点进行专门的微基准测试，用于验证优化效果。

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use memex_core::api::{CompositeToolEventParser, ToolEventRuntime, TOOL_EVENT_PREFIX};
use serde_json::Value;

/// 测试纯JSON反序列化性能（不包含业务逻辑）
fn bench_json_deserialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_deserialize");

    // Gemini格式的典型JSON行
    let tool_use_json = r#"{"type":"tool_use","timestamp":"2025-12-26T12:48:36.765Z","tool_name":"run_shell_command","tool_id":"run_shell_command-1766753316765-e8db","parameters":{"command":"echo hi"}}"#;
    let tool_result_json = r#"{"type":"tool_result","timestamp":"2025-12-26T12:48:38.811Z","tool_id":"run_shell_command-1766753316765-e8db","status":"success","output":""}"#;

    group.bench_function("tool_use_line", |b| {
        b.iter(|| {
            let _: Value = serde_json::from_str(black_box(tool_use_json)).unwrap();
        })
    });

    group.bench_function("tool_result_line", |b| {
        b.iter(|| {
            let _: Value = serde_json::from_str(black_box(tool_result_json)).unwrap();
        })
    });

    group.finish();
}

/// 测试ToolEvent解析器的开销（JSON解析 + 业务逻辑）
fn bench_tool_event_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("tool_event_parsing");

    let tool_use = r#"{"type":"tool_use","timestamp":"2025-12-26T12:48:36.765Z","tool_name":"run_shell_command","tool_id":"run_shell_command-1766753316765-e8db","parameters":{"command":"echo hi"}}"#;
    let tool_result = r#"{"type":"tool_result","timestamp":"2025-12-26T12:48:38.811Z","tool_id":"run_shell_command-1766753316765-e8db","status":"success","output":""}"#;

    group.bench_function("parse_tool_use", |b| {
        b.iter(|| {
            let parser = CompositeToolEventParser::new(TOOL_EVENT_PREFIX);
            let mut rt = ToolEventRuntime::new(parser, None, Some("test-run-id".to_string()));
            // 同步版本的解析测试（模拟热路径）
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async { rt.observe_line(black_box(tool_use)).await });
        })
    });

    group.bench_function("parse_tool_result", |b| {
        b.iter(|| {
            let parser = CompositeToolEventParser::new(TOOL_EVENT_PREFIX);
            let mut rt = ToolEventRuntime::new(parser, None, Some("test-run-id".to_string()));
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async { rt.observe_line(black_box(tool_result)).await });
        })
    });

    group.finish();
}

/// 测试纯文本行跳过性能（早期退出优化验证）
fn bench_text_line_skip(c: &mut Criterion) {
    let mut group = c.benchmark_group("text_line_skip");

    let plain_text = "this is plain text output from the command";
    let almost_json = r#"{"malformed json without closing brace"#;

    group.bench_function("plain_text", |b| {
        b.iter(|| {
            let parser = CompositeToolEventParser::new(TOOL_EVENT_PREFIX);
            let mut rt = ToolEventRuntime::new(parser, None, Some("test-run-id".to_string()));
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async { rt.observe_line(black_box(plain_text)).await });
        })
    });

    group.bench_function("malformed_json", |b| {
        b.iter(|| {
            let parser = CompositeToolEventParser::new(TOOL_EVENT_PREFIX);
            let mut rt = ToolEventRuntime::new(parser, None, Some("test-run-id".to_string()));
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async { rt.observe_line(black_box(almost_json)).await });
        })
    });

    group.finish();
}

/// 测试字符串操作开销（模拟io_pump中的UTF-8转换）
fn bench_string_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("string_operations");

    let bytes = b"this is a typical line of output from a command\n".to_vec();

    group.bench_function("utf8_conversion", |b| {
        b.iter(|| {
            let _s = String::from_utf8_lossy(black_box(&bytes)).to_string();
        })
    });

    group.bench_function("find_newline", |b| {
        b.iter(|| {
            let _pos = black_box(&bytes).iter().position(|&b| b == b'\n');
        })
    });

    // 对比：使用memchr查找换行符（优化后的版本）
    group.bench_function("memchr_newline", |b| {
        b.iter(|| {
            let _pos = memchr::memchr(b'\n', black_box(&bytes));
        })
    });

    group.finish();
}

/// 批量处理测试：模拟真实工作负载
fn bench_batch_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_processing");

    let tool_use = r#"{"type":"tool_use","timestamp":"2025-12-26T12:48:36.765Z","tool_name":"run_shell_command","tool_id":"run_shell_command-1766753316765-e8db","parameters":{"command":"echo hi"}}"#;
    let tool_result = r#"{"type":"tool_result","timestamp":"2025-12-26T12:48:38.811Z","tool_id":"run_shell_command-1766753316765-e8db","status":"success","output":""}"#;

    for count in [100, 500, 1000, 5000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, &count| {
            b.iter(|| {
                let parser = CompositeToolEventParser::new(TOOL_EVENT_PREFIX);
                let mut rt = ToolEventRuntime::new(parser, None, Some("test-run-id".to_string()));
                let runtime = tokio::runtime::Runtime::new().unwrap();
                runtime.block_on(async {
                    for i in 0..count {
                        let line = if i % 2 == 0 { tool_use } else { tool_result };
                        rt.observe_line(black_box(line)).await;
                    }
                });
            })
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_json_deserialize,
    bench_tool_event_parsing,
    bench_text_line_skip,
    bench_string_operations,
    bench_batch_processing
);
criterion_main!(benches);
