//! Unified Executor for Task Dependency Graph (DAG) Execution
//!
//! This module provides a unified execution engine for single-task and multi-task scenarios.
//! It supports:
//! - Task dependency graph construction and validation
//! - Topological sorting for parallel execution stages
//! - Circular dependency detection
//! - Parallel task scheduling with concurrency control
//! - Structured output in both text and JSONL formats
//!
//! # Architecture
//!
//! ```text
//! Vec<StdioTask>
//!   ↓
//! TaskGraph::from_tasks()
//!   ↓
//! TaskGraph { nodes, edges, reverse_edges }
//!   ↓
//! TaskGraph::validate() → detect_cycle(), check_dependencies()
//!   ↓
//! TaskGraph::topological_sort() → Vec<Vec<String>> (execution stages)
//!   ↓
//! ExecutionEngine::execute_stages() → ExecutionResult
//! ```

mod engine;
mod graph;
mod output;
mod progress;
mod scheduler;
pub mod traits;
pub mod types;

pub use engine::{execute_tasks, ExecutionEngine};
pub use graph::TaskGraph;
pub use output::{
    emit_debug, emit_execution_plan, emit_info, emit_run_end, emit_run_start, emit_stage_end,
    emit_stage_start, emit_warning,
};
pub use progress::ProgressMonitor;
pub use scheduler::execute_stage_parallel;
pub use types::{ExecutionOpts, ExecutionResult, TaskResult};
