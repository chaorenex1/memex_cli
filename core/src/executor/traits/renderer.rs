use crate::executor::types::{ExecutionResult, TaskResult};

/// 输出渲染器插件（控制输出格式）
pub trait OutputRendererPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn format(&self) -> &str;
    fn supports_streaming(&self) -> bool {
        false
    }
    fn render(&self, event: &RenderEvent);
}

/// 渲染事件（统一事件类型）
#[derive(Debug, Clone)]
pub enum RenderEvent {
    RunStart {
        run_id: String,
        total_tasks: usize,
        total_stages: usize,
    },
    Plan {
        run_id: String,
        stages: Vec<Vec<String>>,
    },
    StageStart {
        run_id: String,
        stage_id: usize,
        task_ids: Vec<String>,
    },
    TaskStart {
        run_id: String,
        task_id: String,
        stage_id: usize,
    },
    TaskProgress {
        run_id: String,
        task_id: String,
        progress: f32,
        message: Option<String>,
    },
    TaskComplete {
        run_id: String,
        task_id: String,
        result: TaskResult,
    },
    StageEnd {
        run_id: String,
        stage_id: usize,
    },
    RunEnd {
        run_id: String,
        result: ExecutionResult,
    },
}
