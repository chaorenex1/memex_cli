use memex_core::executor::traits::{OutputRendererPlugin, RenderEvent};

pub struct TextRendererPlugin {
    ascii_only: bool,
}

impl TextRendererPlugin {
    pub fn new(ascii_only: bool) -> Self {
        Self { ascii_only }
    }

    fn format_event(&self, event: &RenderEvent) -> String {
        match event {
            RenderEvent::RunStart {
                run_id,
                total_tasks,
                total_stages,
            } => format!(
                "RUN START {} (tasks: {}, stages: {})",
                run_id, total_tasks, total_stages
            ),
            RenderEvent::Plan { run_id, stages } => {
                let mut out = format!("PLAN {}:", run_id);
                for (idx, stage) in stages.iter().enumerate() {
                    out.push_str(&format!("\n  stage {}: {}", idx, stage.join(", ")));
                }
                out
            }
            RenderEvent::StageStart {
                run_id,
                stage_id,
                task_ids,
            } => format!(
                "STAGE START {} (stage {}, tasks: {})",
                run_id,
                stage_id,
                task_ids.len()
            ),
            RenderEvent::TaskStart {
                run_id,
                task_id,
                stage_id,
            } => format!(
                "TASK START {} (stage {}, task {})",
                run_id, stage_id, task_id
            ),
            RenderEvent::TaskProgress {
                run_id,
                task_id,
                progress,
                message,
            } => {
                let mut line = format!(
                    "TASK PROGRESS {} (task {}, {}%)",
                    run_id,
                    task_id,
                    (progress * 100.0) as u32
                );
                if let Some(msg) = message {
                    line.push_str(&format!(": {}", msg));
                }
                line
            }
            RenderEvent::TaskComplete {
                run_id,
                task_id,
                result,
            } => {
                let status = if result.exit_code == 0 {
                    if self.ascii_only {
                        "OK"
                    } else {
                        "SUCCESS"
                    }
                } else if self.ascii_only {
                    "FAIL"
                } else {
                    "FAILED"
                };
                format!(
                    "TASK END {} (task {}, status {}, exit {}, duration {}ms, retries {})",
                    run_id,
                    task_id,
                    status,
                    result.exit_code,
                    result.duration_ms,
                    result.retries_used
                )
            }
            RenderEvent::StageEnd { run_id, stage_id } => {
                format!("STAGE END {} (stage {})", run_id, stage_id)
            }
            RenderEvent::RunEnd { run_id, result } => format!(
                "RUN END {} (completed {}, failed {}, duration {}ms)",
                run_id, result.completed, result.failed, result.duration_ms
            ),
        }
    }
}

impl OutputRendererPlugin for TextRendererPlugin {
    fn name(&self) -> &str {
        "text-renderer"
    }

    fn format(&self) -> &str {
        "text"
    }

    fn render(&self, event: &RenderEvent) {
        println!("{}", self.format_event(event));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memex_core::executor::types::TaskResult;

    #[test]
    fn test_text_renderer_task_complete() {
        let renderer = TextRendererPlugin::new(true);
        let event = RenderEvent::TaskComplete {
            run_id: "run".to_string(),
            task_id: "task".to_string(),
            result: TaskResult {
                task_id: "task".to_string(),
                exit_code: 1,
                duration_ms: 5,
                output: "oops".to_string(),
                error: None,
                retries_used: 2,
            },
        };

        let line = renderer.format_event(&event);
        assert!(line.contains("TASK END"));
        assert!(line.contains("exit 1"));
    }
}
