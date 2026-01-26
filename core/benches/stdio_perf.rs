//! STDIO 协议性能基准测试（Level 5）
//!
//! 使用 Criterion 框架对 STDIO 协议的关键函数进行性能基准测试。

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use memex_core::stdio::{parse_stdio_tasks, FilesEncoding, FilesMode, StdioTask};

/// 生成测试任务输入
fn generate_test_tasks(count: usize) -> String {
    let mut input = String::with_capacity(count * 200);

    for i in 0..count {
        input.push_str("---TASK---\n");
        input.push_str(&format!("id: task-{}\n", i));
        input.push_str("backend: codex\n");
        input.push_str("workdir: .\n");
        input.push_str("stream-format: jsonl\n");
        input.push_str("---CONTENT---\n");
        input.push_str(&format!("这是任务 {} 的内容。\n", i));
        input.push_str("请执行这个任务。\n");
        input.push_str("---END---\n\n");
    }

    input
}

/// 生成包含依赖关系的测试任务
fn generate_tasks_with_deps(count: usize) -> String {
    let mut input = String::new();

    // 第一个任务（无依赖）
    input.push_str("---TASK---\n");
    input.push_str("id: task-0\n");
    input.push_str("backend: codex\n");
    input.push_str("workdir: .\n");
    input.push_str("---CONTENT---\n");
    input.push_str("初始任务\n");
    input.push_str("---END---\n\n");

    // 后续任务（依赖前一个）
    for i in 1..count {
        input.push_str("---TASK---\n");
        input.push_str(&format!("id: task-{}\n", i));
        input.push_str("backend: codex\n");
        input.push_str("workdir: .\n");
        input.push_str(&format!("dependencies: task-{}\n", i - 1));
        input.push_str("---CONTENT---\n");
        input.push_str(&format!("任务 {}\n", i));
        input.push_str("---END---\n\n");
    }

    input
}

/// 基准测试：任务解析性能
fn bench_parse_tasks(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_tasks");

    // 测试不同规模的任务数量
    for size in [10, 50, 100, 500].iter() {
        let input = generate_test_tasks(*size);

        group.bench_with_input(BenchmarkId::new("simple", size), &input, |b, i| {
            b.iter(|| parse_stdio_tasks(black_box(i)))
        });
    }

    group.finish();
}

/// 基准测试：包含依赖关系的任务解析
fn bench_parse_tasks_with_deps(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_tasks_with_deps");

    for size in [10, 50, 100].iter() {
        let input = generate_tasks_with_deps(*size);

        group.bench_with_input(BenchmarkId::new("linear_deps", size), &input, |b, i| {
            b.iter(|| parse_stdio_tasks(black_box(i)))
        });
    }

    group.finish();
}

/// 基准测试：大文本任务解析
fn bench_parse_large_content(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_large_content");

    // 生成包含大量内容的单个任务
    let large_content = "这是一段很长的内容。\n".repeat(1000);
    let mut input = String::new();
    input.push_str("---TASK---\n");
    input.push_str("id: large-task\n");
    input.push_str("backend: codex\n");
    input.push_str("workdir: .\n");
    input.push_str("---CONTENT---\n");
    input.push_str(&large_content);
    input.push_str("---END---\n");

    group.bench_function("1000_lines", |b| {
        b.iter(|| parse_stdio_tasks(black_box(&input)))
    });

    group.finish();
}

/// 基准测试：任务创建（内存分配）
fn bench_task_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("task_creation");

    group.bench_function("create_simple_task", |b| {
        b.iter(|| {
            black_box(StdioTask {
                id: "test-task".to_string(),
                backend: "codex".to_string(),
                workdir: ".".to_string(),
                model: None,
                model_provider: None,
                dependencies: vec![],
                stream_format: "jsonl".to_string(),
                timeout: None,
                retry: None,
                files: vec![],
                files_mode: FilesMode::Auto,
                files_encoding: FilesEncoding::Auto,
                content: "测试内容".to_string(),
                backend_kind: None,
                env_file: None,
                env: None,
                task_level: None,
                resume_run_id: None,
                resume_context: None,
            })
        })
    });

    group.bench_function("create_task_with_files", |b| {
        b.iter(|| {
            black_box(StdioTask {
                id: "test-task".to_string(),
                backend: "codex".to_string(),
                workdir: ".".to_string(),
                model: Some("gpt-5.2".to_string()),
                model_provider: Some("openai".to_string()),
                dependencies: vec!["dep1".to_string(), "dep2".to_string()],
                stream_format: "jsonl".to_string(),
                timeout: Some(30000),
                retry: Some(3),
                files: vec!["file1.txt".to_string(), "file2.rs".to_string()],
                files_mode: FilesMode::Embed,
                files_encoding: FilesEncoding::Utf8,
                content: "测试内容".repeat(100),
                backend_kind: None,
                env_file: None,
                env: None,
                task_level: None,
                resume_run_id: None,
                resume_context: None,
            })
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_parse_tasks,
    bench_parse_tasks_with_deps,
    bench_parse_large_content,
    bench_task_creation
);
criterion_main!(benches);
