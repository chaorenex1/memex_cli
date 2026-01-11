use chrono::Local;

use crate::stdio::{emit_json, JsonlEvent};

use super::types::ExecutionOpts;

/// Emit execution plan (JSONL only)
pub fn emit_execution_plan(opts: &ExecutionOpts, run_id: &str, stages: &[Vec<String>]) {
    if opts.stream_format == "jsonl" {
        let total_tasks: usize = stages.iter().map(|s| s.len()).sum();
        let event = JsonlEvent {
            v: 1,
            event_type: "executor.plan".to_string(),
            ts: Local::now().to_rfc3339(),
            run_id: run_id.to_string(),
            task_id: None,
            action: None,
            args: None,
            output: None,
            error: None,
            code: None,
            progress: None,
            metadata: Some(serde_json::json!({
                "stages": stages,
                "total_tasks": total_tasks,
            })),
        };
        emit_json(&event);
    } else if opts.verbose {
        println!("📋 Execution Plan:");
        for (i, stage) in stages.iter().enumerate() {
            println!("  Stage {}: {}", i, stage.join(", "));
        }
        println!();
    }
}

/// Emit stage start event
pub fn emit_stage_start(opts: &ExecutionOpts, run_id: &str, stage_id: usize, task_ids: &[String]) {
    if opts.stream_format == "jsonl" {
        let event = JsonlEvent {
            v: 1,
            event_type: "stage.start".to_string(),
            ts: Local::now().to_rfc3339(),
            run_id: run_id.to_string(),
            task_id: None,
            action: None,
            args: None,
            output: None,
            error: None,
            code: None,
            progress: None,
            metadata: Some(serde_json::json!({
                "stage_id": stage_id,
                "tasks": task_ids,
            })),
        };
        emit_json(&event);
    } else if opts.verbose && !opts.quiet {
        println!("▶ Stage {} ({} tasks)", stage_id, task_ids.len());
    }
}

/// Emit stage end event
pub fn emit_stage_end(opts: &ExecutionOpts, run_id: &str, stage_id: usize) {
    if opts.stream_format == "jsonl" {
        let event = JsonlEvent {
            v: 1,
            event_type: "stage.end".to_string(),
            ts: Local::now().to_rfc3339(),
            run_id: run_id.to_string(),
            task_id: None,
            action: None,
            args: None,
            output: None,
            error: None,
            code: None,
            progress: None,
            metadata: Some(serde_json::json!({
                "stage_id": stage_id,
            })),
        };
        emit_json(&event);
    }
}

/// Emit task start event
pub fn emit_task_start(opts: &ExecutionOpts, run_id: &str, task_id: &str, stage_id: usize) {
    if opts.stream_format == "jsonl" {
        let event = JsonlEvent {
            v: 1,
            event_type: "task.start".to_string(),
            ts: Local::now().to_rfc3339(),
            run_id: run_id.to_string(),
            task_id: Some(task_id.to_string()),
            action: None,
            args: None,
            output: None,
            error: None,
            code: None,
            progress: None,
            metadata: Some(serde_json::json!({
                "stage_id": stage_id,
            })),
        };
        emit_json(&event);
    } else if opts.verbose && !opts.quiet {
        println!("  ⏳ Starting task: {}", task_id);
    }
}

/// Emit task complete event (Protocol 2.3 - using task.end)
pub fn emit_task_complete(
    opts: &ExecutionOpts,
    run_id: &str,
    task_id: &str,
    exit_code: i32,
    duration_ms: u64,
    retries_used: u32,
) {
    if opts.stream_format == "jsonl" {
        let event = JsonlEvent {
            v: 1,
            event_type: "task.end".to_string(),
            ts: Local::now().to_rfc3339(),
            run_id: run_id.to_string(),
            task_id: Some(task_id.to_string()),
            action: None,
            args: None,
            output: None,
            error: None,
            code: Some(exit_code),
            progress: None,
            metadata: Some(serde_json::json!({
                "duration_ms": duration_ms,
                "retries_used": retries_used,
                "success": exit_code == 0,
            })),
        };
        emit_json(&event);
    } else if opts.verbose && !opts.quiet {
        let icon = if exit_code == 0 { "✅" } else { "❌" };
        let retry_info = if retries_used > 0 {
            format!(" (retries: {})", retries_used)
        } else {
            String::new()
        };
        println!(
            "  {} Task {}: {}ms{}",
            icon, task_id, duration_ms, retry_info
        );
    }
}

/// Emit progress update event
pub fn emit_progress_update(
    opts: &ExecutionOpts,
    run_id: &str,
    completed: usize,
    total: usize,
    current_stage: usize,
    total_stages: usize,
) {
    let percentage = if total > 0 {
        (completed as f64 / total as f64 * 100.0) as u8
    } else {
        0
    };

    if opts.stream_format == "jsonl" {
        let event = JsonlEvent {
            v: 1,
            event_type: "executor.progress".to_string(),
            ts: Local::now().to_rfc3339(),
            run_id: run_id.to_string(),
            task_id: None,
            action: None,
            args: None,
            output: None,
            error: None,
            code: None,
            progress: Some(percentage),
            metadata: Some(serde_json::json!({
                "completed": completed,
                "total": total,
                "current_stage": current_stage,
                "total_stages": total_stages,
            })),
        };
        emit_json(&event);
    } else if !opts.quiet {
        println!(
            "📊 Progress: {}/{} tasks ({}%) - Stage {}/{}",
            completed,
            total,
            percentage,
            current_stage + 1,
            total_stages
        );
    }
}

/// Emit run start event (Protocol 2.3.1)
pub fn emit_run_start(opts: &ExecutionOpts, run_id: &str, total_tasks: usize, total_stages: usize) {
    if opts.stream_format == "jsonl" {
        let event = JsonlEvent {
            v: 1,
            event_type: "run.start".to_string(),
            ts: Local::now().to_rfc3339(),
            run_id: run_id.to_string(),
            task_id: None,
            action: None,
            args: None,
            output: None,
            error: None,
            code: None,
            progress: None,
            metadata: Some(serde_json::json!({
                "total_tasks": total_tasks,
                "total_stages": total_stages,
            })),
        };
        emit_json(&event);
    } else if !opts.quiet {
        println!(
            "🚀 Starting execution: {} tasks in {} stages",
            total_tasks, total_stages
        );
    }
}

/// Emit run end event (Protocol 2.3.9)
pub fn emit_run_end(opts: &ExecutionOpts, run_id: &str, result: &super::types::ExecutionResult) {
    if opts.stream_format == "jsonl" {
        let event = JsonlEvent {
            v: 1,
            event_type: "run.end".to_string(),
            ts: Local::now().to_rfc3339(),
            run_id: run_id.to_string(),
            task_id: None,
            action: None,
            args: None,
            output: None,
            error: None,
            code: Some(if result.failed == 0 { 0 } else { 1 }),
            progress: None,
            metadata: Some(serde_json::json!({
                "total_tasks": result.total_tasks,
                "completed": result.completed,
                "failed": result.failed,
                "duration_ms": result.duration_ms,
            })),
        };
        emit_json(&event);
    } else if !opts.quiet {
        let icon = if result.failed == 0 { "✅" } else { "❌" };
        println!(
            "\n{} Execution finished: {}/{} tasks completed in {}ms",
            icon, result.completed, result.total_tasks, result.duration_ms
        );
    }
}

/// Emit warning event (Protocol 2.3)
pub fn emit_warning(opts: &ExecutionOpts, run_id: &str, task_id: Option<&str>, message: &str) {
    if opts.stream_format == "jsonl" {
        let event = JsonlEvent {
            v: 1,
            event_type: "warning".to_string(),
            ts: Local::now().to_rfc3339(),
            run_id: run_id.to_string(),
            task_id: task_id.map(|s| s.to_string()),
            action: None,
            args: None,
            output: Some(message.to_string()),
            error: None,
            code: None,
            progress: None,
            metadata: None,
        };
        emit_json(&event);
    } else if !opts.quiet {
        let task_prefix = task_id.map(|id| format!("[{}] ", id)).unwrap_or_default();
        println!("⚠️  {}{}", task_prefix, message);
    }
}

/// Emit info event (Protocol 2.3)
pub fn emit_info(opts: &ExecutionOpts, run_id: &str, task_id: Option<&str>, message: &str) {
    if opts.stream_format == "jsonl" {
        let event = JsonlEvent {
            v: 1,
            event_type: "info".to_string(),
            ts: Local::now().to_rfc3339(),
            run_id: run_id.to_string(),
            task_id: task_id.map(|s| s.to_string()),
            action: None,
            args: None,
            output: Some(message.to_string()),
            error: None,
            code: None,
            progress: None,
            metadata: None,
        };
        emit_json(&event);
    } else if opts.verbose && !opts.quiet {
        let task_prefix = task_id.map(|id| format!("[{}] ", id)).unwrap_or_default();
        println!("ℹ️  {}{}", task_prefix, message);
    }
}

/// Emit debug event (Protocol 2.3)
pub fn emit_debug(opts: &ExecutionOpts, run_id: &str, task_id: Option<&str>, message: &str) {
    if opts.stream_format == "jsonl" {
        let event = JsonlEvent {
            v: 1,
            event_type: "debug".to_string(),
            ts: Local::now().to_rfc3339(),
            run_id: run_id.to_string(),
            task_id: task_id.map(|s| s.to_string()),
            action: None,
            args: None,
            output: Some(message.to_string()),
            error: None,
            code: None,
            progress: None,
            metadata: None,
        };
        emit_json(&event);
    }
    // Debug events only output in jsonl mode
}
