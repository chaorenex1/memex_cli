use std::collections::HashMap;

/// Result of executing a task graph
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Total number of tasks in the graph
    pub total_tasks: usize,

    /// Number of tasks completed (may be less than total if failed early)
    pub completed: usize,

    /// Number of tasks that failed (exit code != 0)
    pub failed: usize,

    /// Total execution duration in milliseconds
    pub duration_ms: u64,

    /// Individual task results (task_id -> TaskResult)
    pub task_results: HashMap<String, TaskResult>,

    /// Execution stages (for debugging)
    pub stages: Vec<Vec<String>>,
}

/// Result of executing a single task
#[derive(Debug, Clone)]
pub struct TaskResult {
    /// Task identifier
    pub task_id: String,

    /// Exit code (0 = success, non-zero = failure)
    pub exit_code: i32,

    /// Execution duration in milliseconds
    pub duration_ms: u64,

    /// Captured output (may be truncated)
    pub output: String,

    /// Error message (if any)
    pub error: Option<String>,

    /// Number of retries used
    pub retries_used: u32,
}
