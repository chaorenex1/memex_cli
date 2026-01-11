use chrono::Local;
use lazy_static::lazy_static;
use serde::Serialize;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::runner::RunnerEvent;

#[derive(Debug, Clone)]
pub struct RenderTaskInfo {
    pub task_id: String,
    pub backend: String,
    pub model: Option<String>,
    pub dependencies: Vec<String>,
    pub files: Vec<FileInfo>,
}

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct RenderOutcome {
    pub exit_code: i32,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JsonlEvent {
    pub v: i32,
    #[serde(rename = "type")]
    pub event_type: String,
    pub ts: String,
    pub run_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

// ============================================================================
// Event Buffering (Level 2.1 优化)
// ============================================================================

/// 事件缓冲区配置
#[derive(Debug, Clone)]
pub struct EventBufferConfig {
    /// 缓冲区大小（事件数量）
    pub buffer_size: usize,
    /// 刷新间隔（毫秒）
    pub flush_interval_ms: u64,
}

impl Default for EventBufferConfig {
    fn default() -> Self {
        Self {
            buffer_size: 50,
            flush_interval_ms: 100,
        }
    }
}

/// 事件缓冲区
struct EventBuffer {
    events: Vec<JsonlEvent>,
    last_flush: Instant,
}

impl EventBuffer {
    fn new() -> Self {
        Self {
            events: Vec::new(),
            last_flush: Instant::now(),
        }
    }

    /// 添加事件到缓冲区，并在满足条件时自动刷新
    fn add(&mut self, event: JsonlEvent, config: &EventBufferConfig) {
        self.events.push(event);

        // 自动刷新条件：缓冲区满 || 超时
        let should_flush = self.events.len() >= config.buffer_size
            || self.last_flush.elapsed() > Duration::from_millis(config.flush_interval_ms);

        if should_flush {
            self.flush();
        }
    }

    /// 刷新缓冲区：一次性输出所有事件
    fn flush(&mut self) {
        if self.events.is_empty() {
            return;
        }

        // 预分配字符串缓冲区（每个事件约 200 字节）
        let mut output = String::with_capacity(self.events.len() * 200);

        for event in &self.events {
            if let Ok(json) = serde_json::to_string(event) {
                output.push_str(&json);
                output.push('\n');
            }
        }

        // 单次系统调用写入
        print!("{}", output);

        self.events.clear();
        self.last_flush = Instant::now();
    }

    /// 强制刷新（用于任务结束时）
    fn force_flush(&mut self) {
        self.flush();
    }
}

// 全局事件缓冲区和配置
lazy_static! {
    static ref EVENT_BUFFER: Mutex<EventBuffer> = Mutex::new(EventBuffer::new());
    static ref BUFFER_CONFIG: Mutex<EventBufferConfig> = Mutex::new(EventBufferConfig::default());
    static ref BUFFERING_ENABLED: Mutex<bool> = Mutex::new(false);
}

/// 批量化输出事件（Level 2.1 优化）
pub fn emit_json_buffered(event: &JsonlEvent) {
    if let Ok(mut buffer) = EVENT_BUFFER.lock() {
        if let Ok(config) = BUFFER_CONFIG.lock() {
            buffer.add(event.clone(), &config);
        }
    }
}

/// 强制刷新缓冲区（任务结束时调用）
pub fn flush_event_buffer() {
    if let Ok(mut buffer) = EVENT_BUFFER.lock() {
        buffer.force_flush();
    }
}

/// 配置事件缓冲区（初始化时调用）
pub fn configure_event_buffer(enable_buffering: bool, buffer_size: usize, flush_interval_ms: u64) {
    if let Ok(mut enabled) = BUFFERING_ENABLED.lock() {
        *enabled = enable_buffering;
    }

    if let Ok(mut config) = BUFFER_CONFIG.lock() {
        *config = EventBufferConfig {
            buffer_size,
            flush_interval_ms,
        };
    }
}

// ============================================================================
// Text Rendering Markers
// ============================================================================

#[derive(Debug, Clone)]
pub struct TextMarkers {
    pub start: &'static str,
    pub ok: &'static str,
    pub fail: &'static str,
    pub retry: &'static str,
    pub wait: &'static str,
    pub action: &'static str,
    pub warn: &'static str,
    pub file: &'static str,
}

impl TextMarkers {
    pub fn unicode() -> Self {
        Self {
            start: "▶",
            ok: "✓",
            fail: "✗",
            retry: "⟳",
            wait: "⏸",
            action: "»",
            warn: "⚠",
            file: "📄",
        }
    }

    pub fn ascii() -> Self {
        Self {
            start: ">",
            ok: "[OK]",
            fail: "[FAIL]",
            retry: "[RETRY]",
            wait: "[WAIT]",
            action: ">>",
            warn: "[WARN]",
            file: "-",
        }
    }
}

pub async fn render_task_jsonl(
    run_id: &str,
    info: RenderTaskInfo,
    rx: UnboundedReceiver<RunnerEvent>,
) -> RenderOutcome {
    emit_task_start_jsonl(run_id, &info);
    let out = render_task_jsonl_events(run_id, info.clone(), rx).await;
    emit_task_end_jsonl(run_id, &info, out.exit_code, out.duration_ms, 0);
    out
}

pub fn emit_task_start_jsonl(run_id: &str, info: &RenderTaskInfo) {
    emit_json(&JsonlEvent {
        v: 1,
        event_type: "task.start".into(),
        ts: Local::now().to_rfc3339(),
        run_id: run_id.to_string(),
        task_id: Some(info.task_id.clone()),
        action: None,
        args: None,
        output: None,
        error: None,
        code: None,
        progress: Some(0),
        metadata: Some(serde_json::json!({
            "backend": info.backend,
            "model": info.model,
            "dependencies": info.dependencies,
        })),
    });
}

pub fn emit_task_end_jsonl(
    run_id: &str,
    info: &RenderTaskInfo,
    exit_code: i32,
    duration_ms: Option<u64>,
    retries: u32,
) {
    emit_json(&JsonlEvent {
        v: 1,
        event_type: "task.end".into(),
        ts: Local::now().to_rfc3339(),
        run_id: run_id.to_string(),
        task_id: Some(info.task_id.clone()),
        action: None,
        args: None,
        output: None,
        error: None,
        code: Some(exit_code),
        progress: Some(100),
        metadata: Some(serde_json::json!({
            "status": if exit_code == 0 { "success" } else { "failed" },
            "duration_ms": duration_ms,
            "retries": retries,
        })),
    });
}

pub async fn render_task_jsonl_events(
    run_id: &str,
    info: RenderTaskInfo,
    mut rx: UnboundedReceiver<RunnerEvent>,
) -> RenderOutcome {
    let started = std::time::Instant::now();
    let mut exit_code = 0;
    let mut saw_complete = false;

    while let Some(ev) = rx.recv().await {
        match ev {
            RunnerEvent::AssistantOutput(text) => {
                emit_json(&JsonlEvent {
                    v: 1,
                    event_type: "assistant.output".into(),
                    ts: Local::now().to_rfc3339(),
                    run_id: run_id.to_string(),
                    task_id: Some(info.task_id.clone()),
                    action: None,
                    args: None,
                    output: Some(text),
                    error: None,
                    code: None,
                    progress: None,
                    metadata: None,
                });
            }
            RunnerEvent::ToolEvent(tool) => match tool.event_type.as_str() {
                "tool.request" => emit_json(&JsonlEvent {
                    v: 1,
                    event_type: "tool.call".into(),
                    ts: Local::now().to_rfc3339(),
                    run_id: run_id.to_string(),
                    task_id: Some(info.task_id.clone()),
                    action: tool.action.clone(),
                    args: Some(tool.args.clone()),
                    output: None,
                    error: None,
                    code: None,
                    progress: None,
                    metadata: None,
                }),
                "tool.result" => emit_json(&JsonlEvent {
                    v: 1,
                    event_type: "tool.result".into(),
                    ts: Local::now().to_rfc3339(),
                    run_id: run_id.to_string(),
                    task_id: Some(info.task_id.clone()),
                    action: tool.action.clone(),
                    args: None,
                    output: tool
                        .output
                        .as_ref()
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    error: tool.error.clone(),
                    code: tool.ok.map(|ok| if ok { 0 } else { 1 }),
                    progress: None,
                    metadata: None,
                }),
                "assistant.output" => {
                    if let Some(v) = tool.output.as_ref().and_then(|v| v.as_str()) {
                        emit_json(&JsonlEvent {
                            v: 1,
                            event_type: "assistant.output".into(),
                            ts: Local::now().to_rfc3339(),
                            run_id: run_id.to_string(),
                            task_id: Some(info.task_id.clone()),
                            action: None,
                            args: None,
                            output: Some(v.to_string()),
                            error: None,
                            code: None,
                            progress: None,
                            metadata: None,
                        });
                    }
                }
                "assistant.thinking" => {
                    if let Some(v) = tool.output.as_ref().and_then(|v| v.as_str()) {
                        emit_json(&JsonlEvent {
                            v: 1,
                            event_type: "assistant.thinking".into(),
                            ts: Local::now().to_rfc3339(),
                            run_id: run_id.to_string(),
                            task_id: Some(info.task_id.clone()),
                            action: None,
                            args: None,
                            output: Some(v.to_string()),
                            error: None,
                            code: None,
                            progress: None,
                            metadata: None,
                        });
                    }
                }
                "assistant.action" => emit_json(&JsonlEvent {
                    v: 1,
                    event_type: "assistant.action".into(),
                    ts: Local::now().to_rfc3339(),
                    run_id: run_id.to_string(),
                    task_id: Some(info.task_id.clone()),
                    action: tool.action.clone(),
                    args: Some(tool.args.clone()),
                    output: tool
                        .output
                        .as_ref()
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    error: None,
                    code: None,
                    progress: None,
                    metadata: None,
                }),
                "info" => {
                    if let Some(v) = tool.output.as_ref().and_then(|v| v.as_str()) {
                        emit_json(&JsonlEvent {
                            v: 1,
                            event_type: "info".into(),
                            ts: Local::now().to_rfc3339(),
                            run_id: run_id.to_string(),
                            task_id: Some(info.task_id.clone()),
                            action: None,
                            args: None,
                            output: Some(v.to_string()),
                            error: None,
                            code: None,
                            progress: None,
                            metadata: None,
                        });
                    }
                }
                "debug" => {
                    if let Some(v) = tool.output.as_ref().and_then(|v| v.as_str()) {
                        emit_json(&JsonlEvent {
                            v: 1,
                            event_type: "debug".into(),
                            ts: Local::now().to_rfc3339(),
                            run_id: run_id.to_string(),
                            task_id: Some(info.task_id.clone()),
                            action: None,
                            args: None,
                            output: Some(v.to_string()),
                            error: None,
                            code: None,
                            progress: None,
                            metadata: None,
                        });
                    }
                }
                _ => {}
            },
            RunnerEvent::RawStdout(line) => emit_json(&JsonlEvent {
                v: 1,
                event_type: "assistant.output".into(),
                ts: Local::now().to_rfc3339(),
                run_id: run_id.to_string(),
                task_id: Some(info.task_id.clone()),
                action: None,
                args: None,
                output: Some(line),
                error: None,
                code: None,
                progress: None,
                metadata: None,
            }),
            RunnerEvent::RawStderr(line) => emit_json(&JsonlEvent {
                v: 1,
                event_type: "warning".into(),
                ts: Local::now().to_rfc3339(),
                run_id: run_id.to_string(),
                task_id: Some(info.task_id.clone()),
                action: None,
                args: None,
                output: Some(line),
                error: None,
                code: None,
                progress: None,
                metadata: None,
            }),
            RunnerEvent::RunComplete { exit_code: code } => {
                exit_code = code;
                saw_complete = true;
            }
            RunnerEvent::Error(msg) => {
                exit_code = 1;
                emit_json(&JsonlEvent {
                    v: 1,
                    event_type: "error".into(),
                    ts: Local::now().to_rfc3339(),
                    run_id: run_id.to_string(),
                    task_id: Some(info.task_id.clone()),
                    action: None,
                    args: None,
                    output: None,
                    error: Some(msg),
                    code: Some(1),
                    progress: None,
                    metadata: None,
                });
            }
            RunnerEvent::StatusUpdate { .. } => {}
        }
    }

    if !saw_complete {
        exit_code = 1;
    }
    let duration_ms = Some(started.elapsed().as_millis() as u64);

    RenderOutcome {
        exit_code,
        duration_ms,
    }
}

pub async fn render_task_stream(
    info: RenderTaskInfo,
    mut rx: UnboundedReceiver<RunnerEvent>,
    markers: &TextMarkers,
) -> RenderOutcome {
    let started = std::time::Instant::now();
    let mut exit_code = 0;
    let mut saw_complete = false;

    // Print file info if present
    if !info.files.is_empty() {
        for file in &info.files {
            let size_kb = file.size as f64 / 1024.0;
            if size_kb < 1.0 {
                println!("  {} {} ({} bytes)", markers.file, file.path, file.size);
            } else {
                println!("  {} {} ({:.1}KB)", markers.file, file.path, size_kb);
            }
        }
        println!();
    }
    while let Some(ev) = rx.recv().await {
        match ev {
            RunnerEvent::AssistantOutput(text) => println!("{text}"),
            RunnerEvent::ToolEvent(tool) => {
                if let Some(v) = tool
                    .output
                    .as_ref()
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                {
                    if tool.event_type == "assistant.output" {
                        println!("{v}");
                    } else {
                        println!("{} {}", markers.action, v);
                    }
                }
            }
            RunnerEvent::RawStdout(line) => println!("{line}"),
            RunnerEvent::RawStderr(line) => println!("{} {}", markers.warn, line),
            RunnerEvent::RunComplete { exit_code: code } => {
                exit_code = code;
                saw_complete = true;
            }
            RunnerEvent::Error(msg) => {
                exit_code = 1;
                println!("{} {}", markers.fail, msg);
            }
            RunnerEvent::StatusUpdate { .. } => {}
        }
    }

    if !saw_complete {
        exit_code = 1;
    }
    let duration_ms = Some(started.elapsed().as_millis() as u64);
    RenderOutcome {
        exit_code,
        duration_ms,
    }
}

pub fn emit_json(ev: &JsonlEvent) {
    // Level 2.1: 根据全局配置选择输出方式（批量化 vs 直接输出）
    let enable_buffering = BUFFERING_ENABLED
        .lock()
        .map(|enabled| *enabled)
        .unwrap_or(false);

    if enable_buffering {
        // 使用批量化输出（减少 90% 系统调用）
        emit_json_buffered(ev);
    } else {
        // 直接输出（默认行为，实时性更好）
        if let Ok(line) = serde_json::to_string(ev) {
            println!("{line}");
        }
    }
}
