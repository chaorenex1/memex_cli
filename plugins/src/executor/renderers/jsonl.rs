use chrono::Local;
use memex_core::executor::traits::{OutputRendererPlugin, RenderEvent};
use serde_json::{json, Value};

pub struct JsonlRendererPlugin {
    pretty_print: bool,
}

impl JsonlRendererPlugin {
    pub fn new(pretty_print: bool) -> Self {
        Self { pretty_print }
    }

    fn event_to_json(&self, event: &RenderEvent) -> Value {
        let ts = Local::now().to_rfc3339();
        match event {
            RenderEvent::RunStart {
                run_id,
                total_tasks,
                total_stages,
            } => json!({
                "v": 1,
                "event_type": "run.start",
                "ts": ts,
                "run_id": run_id,
                "metadata": {
                    "total_tasks": total_tasks,
                    "total_stages": total_stages,
                }
            }),
            RenderEvent::Plan { run_id, stages } => {
                let total_tasks: usize = stages.iter().map(|s| s.len()).sum();
                json!({
                    "v": 1,
                    "event_type": "executor.plan",
                    "ts": ts,
                    "run_id": run_id,
                    "metadata": {
                        "stages": stages,
                        "total_tasks": total_tasks,
                    }
                })
            }
            RenderEvent::StageStart {
                run_id,
                stage_id,
                task_ids,
            } => json!({
                "v": 1,
                "event_type": "stage.start",
                "ts": ts,
                "run_id": run_id,
                "metadata": {
                    "stage_id": stage_id,
                    "tasks": task_ids,
                }
            }),
            RenderEvent::TaskStart {
                run_id,
                task_id,
                stage_id,
            } => json!({
                "v": 1,
                "event_type": "task.start",
                "ts": ts,
                "run_id": run_id,
                "task_id": task_id,
                "metadata": {
                    "stage_id": stage_id,
                }
            }),
            RenderEvent::TaskProgress {
                run_id,
                task_id,
                progress,
                message,
            } => json!({
                "v": 1,
                "event_type": "executor.progress",
                "ts": ts,
                "run_id": run_id,
                "task_id": task_id,
                "progress": progress,
                "metadata": {
                    "message": message,
                }
            }),
            RenderEvent::TaskComplete {
                run_id,
                task_id,
                result,
            } => json!({
                "v": 1,
                "event_type": "task.end",
                "ts": ts,
                "run_id": run_id,
                "task_id": task_id,
                "code": result.exit_code,
                "metadata": {
                    "duration_ms": result.duration_ms,
                    "retries_used": result.retries_used,
                    "success": result.exit_code == 0,
                }
            }),
            RenderEvent::StageEnd { run_id, stage_id } => json!({
                "v": 1,
                "event_type": "stage.end",
                "ts": ts,
                "run_id": run_id,
                "metadata": {
                    "stage_id": stage_id,
                }
            }),
            RenderEvent::RunEnd { run_id, result } => json!({
                "v": 1,
                "event_type": "run.end",
                "ts": ts,
                "run_id": run_id,
                "metadata": {
                    "total_tasks": result.total_tasks,
                    "completed": result.completed,
                    "failed": result.failed,
                    "duration_ms": result.duration_ms,
                }
            }),
        }
    }
}

impl OutputRendererPlugin for JsonlRendererPlugin {
    fn name(&self) -> &str {
        "jsonl-renderer"
    }

    fn format(&self) -> &str {
        "jsonl"
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn render(&self, event: &RenderEvent) {
        let value = self.event_to_json(event);
        if self.pretty_print {
            println!("{}", serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".into()));
        } else {
            println!("{}", serde_json::to_string(&value).unwrap_or_else(|_| "{}".into()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memex_core::executor::types::{ExecutionResult, TaskResult};

    #[test]
    fn test_jsonl_renderer_event_type() {
        let renderer = JsonlRendererPlugin::new(false);
        let event = RenderEvent::RunStart {
            run_id: "run".to_string(),
            total_tasks: 2,
            total_stages: 1,
        };

        let value = renderer.event_to_json(&event);
        assert_eq!(value["event_type"], "run.start");
    }

    #[test]
    fn test_jsonl_renderer_task_complete() {
        let renderer = JsonlRendererPlugin::new(false);
        let event = RenderEvent::TaskComplete {
            run_id: "run".to_string(),
            task_id: "task".to_string(),
            result: TaskResult {
                task_id: "task".to_string(),
                exit_code: 0,
                duration_ms: 12,
                output: "ok".to_string(),
                error: None,
                retries_used: 1,
            },
        };

        let value = renderer.event_to_json(&event);
        assert_eq!(value["event_type"], "task.end");
        assert_eq!(value["metadata"]["retries_used"], 1);
    }

    #[test]
    fn test_jsonl_renderer_run_end() {
        let renderer = JsonlRendererPlugin::new(false);
        let event = RenderEvent::RunEnd {
            run_id: "run".to_string(),
            result: ExecutionResult {
                total_tasks: 3,
                completed: 3,
                failed: 0,
                duration_ms: 100,
                task_results: Default::default(),
                stages: Vec::new(),
            },
        };

        let value = renderer.event_to_json(&event);
        assert_eq!(value["metadata"]["total_tasks"], 3);
    }
}
