use std::collections::HashMap;
use std::sync::Arc;

use futures::stream::FuturesUnordered;
use futures::StreamExt;
use tokio::sync::Semaphore;

use crate::error::ExecutorError;

use super::graph::TaskGraph;
use super::types::{TaskLike, TaskResult};

/// Execute a single stage of tasks in parallel
///
/// # Arguments
///
/// * `task_ids` - List of task IDs to execute in this stage
/// * `graph` - Task dependency graph
/// * `max_concurrency` - Maximum number of concurrent tasks
/// * `executor_fn` - Async function to execute a single task
///
/// # Returns
///
/// Map of task_id -> TaskResult for all tasks in this stage
pub async fn execute_stage_parallel<T, F, Fut>(
    task_ids: &[String],
    graph: &TaskGraph<T>,
    max_concurrency: usize,
    executor_fn: F,
) -> Result<HashMap<String, TaskResult>, ExecutorError>
where
    T: TaskLike,
    F: Fn(String) -> Fut + Clone + Send + 'static,
    Fut: std::future::Future<Output = Result<TaskResult, ExecutorError>> + Send,
{
    let sem = Arc::new(Semaphore::new(max_concurrency));
    let mut futs: FuturesUnordered<_> = FuturesUnordered::new();

    for id in task_ids {
        let Some(task) = graph.nodes.get(id) else {
            continue;
        };

        let task_id = task.id().to_string();
        let sem = sem.clone();
        let executor = executor_fn.clone();

        futs.push(async move {
            let _permit = sem
                .acquire_owned()
                .await
                .map_err(|_| ExecutorError::Runner("semaphore closed unexpectedly".into()))?;

            executor(task_id).await
        });
    }

    let mut results: HashMap<String, TaskResult> = HashMap::new();

    while let Some(res) = futs.next().await {
        let task_result = res?;
        results.insert(task_result.task_id.clone(), task_result);
    }

    Ok(results)
}
